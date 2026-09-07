//! The wire format of `POST /api/ext/v1/clipboard`: JSON with base64 image
//! data, or multipart with the image as a binary part above
//! [`super::MULTIPART_THRESHOLD`].

use super::model::{ClipboardItem, ClipboardPayload, ImageFormat, PayloadKind, hex};
use super::sensitive::looks_sensitive;
use crate::protocol::Fingerprint;
use base64::Engine;
use bytes::Bytes;
use serde::{Deserialize, Serialize};

/// The JSON body (or the `item` part of a multipart body).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardItemDto {
    pub id: String,
    pub origin_device: String,
    /// Unix time in milliseconds.
    pub created_at: i64,
    /// Hex SHA-256 of the content.
    pub content_hash: String,
    pub kind: PayloadKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<TextDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<ImageDto>,
    /// Sender's hint that the text looks like a secret and should not be
    /// kept in the receiver's history.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub sensitive: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextDto {
    pub plain: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rtf: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageDto {
    pub format: ImageFormat,
    pub width: u32,
    pub height: u32,
    /// Base64 image bytes; absent when the bytes travel as a multipart part.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum WireError {
    #[error("clipboard item of kind {0} cannot be sent through the clipboard endpoint")]
    UnsupportedKind(PayloadKind),

    #[error("invalid clipboard item: {0}")]
    Invalid(String),
}

/// How an item travels: JSON only, or JSON plus a binary image part.
pub enum Encoded {
    Json(ClipboardItemDto),
    Multipart {
        item: ClipboardItemDto,
        image: Bytes,
        mime: &'static str,
    },
}

/// Encodes `item` for sending. Images above `multipart_threshold` bytes go
/// as a multipart part instead of base64.
pub fn encode(item: &ClipboardItem, multipart_threshold: usize) -> Result<Encoded, WireError> {
    let mut dto = ClipboardItemDto {
        id: item.id.clone(),
        origin_device: item.origin_device.to_string(),
        created_at: item.created_at,
        content_hash: item.hash_hex(),
        kind: item.payload.kind(),
        text: None,
        image: None,
        sensitive: false,
    };
    match &item.payload {
        ClipboardPayload::Text { plain, html, rtf } => {
            dto.sensitive = looks_sensitive(plain);
            dto.text = Some(TextDto {
                plain: plain.clone(),
                html: html.clone(),
                rtf: rtf.clone(),
            });
            Ok(Encoded::Json(dto))
        }
        ClipboardPayload::Image {
            format,
            bytes,
            width,
            height,
        } => {
            let mut image = ImageDto {
                format: *format,
                width: *width,
                height: *height,
                data: None,
            };
            if bytes.len() > multipart_threshold {
                dto.image = Some(image);
                Ok(Encoded::Multipart {
                    item: dto,
                    image: bytes.clone(),
                    mime: format.mime(),
                })
            } else {
                image.data = Some(base64::engine::general_purpose::STANDARD.encode(bytes));
                dto.image = Some(image);
                Ok(Encoded::Json(dto))
            }
        }
        ClipboardPayload::Files { .. } => Err(WireError::UnsupportedKind(PayloadKind::Files)),
    }
}

/// Decodes a received item; `image_bytes` is the multipart image part when
/// there was one. Verifies the content hash.
pub fn decode(
    dto: ClipboardItemDto,
    image_bytes: Option<Bytes>,
) -> Result<(ClipboardItem, bool), WireError> {
    let payload = match dto.kind {
        PayloadKind::Text => {
            let text = dto
                .text
                .ok_or_else(|| WireError::Invalid("missing text".into()))?;
            ClipboardPayload::Text {
                plain: text.plain,
                html: text.html,
                rtf: text.rtf,
            }
        }
        PayloadKind::Image => {
            let image = dto
                .image
                .ok_or_else(|| WireError::Invalid("missing image".into()))?;
            let bytes = match (image_bytes, image.data) {
                (Some(bytes), _) => bytes,
                (None, Some(data)) => Bytes::from(
                    base64::engine::general_purpose::STANDARD
                        .decode(data)
                        .map_err(|err| WireError::Invalid(format!("bad base64: {err}")))?,
                ),
                (None, None) => return Err(WireError::Invalid("missing image data".into())),
            };
            ClipboardPayload::Image {
                format: image.format,
                bytes,
                width: image.width,
                height: image.height,
            }
        }
        PayloadKind::Files => return Err(WireError::UnsupportedKind(PayloadKind::Files)),
    };
    let content_hash = payload.content_hash();
    if !hex(&content_hash).eq_ignore_ascii_case(&dto.content_hash) {
        return Err(WireError::Invalid("content hash mismatch".into()));
    }
    Ok((
        ClipboardItem {
            id: dto.id,
            origin_device: Fingerprint::parse(&dto.origin_device),
            created_at: dto.created_at,
            payload,
            content_hash,
        },
        dto.sensitive,
    ))
}

#[cfg(test)]
#[allow(clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn text_and_small_images_round_trip_as_json() {
        let origin = Fingerprint::parse("AB");
        let text = ClipboardItem::new(
            origin.clone(),
            ClipboardPayload::Text {
                plain: "hello".into(),
                html: Some("<p>hello</p>".into()),
                rtf: None,
            },
        );
        let Encoded::Json(dto) = encode(&text, 512).unwrap() else {
            panic!("text must be json");
        };
        let json = serde_json::to_string(&dto).unwrap();
        let back: ClipboardItemDto = serde_json::from_str(&json).unwrap();
        let (decoded, sensitive) = decode(back, None).unwrap();
        assert_eq!(decoded, text);
        assert!(!sensitive);

        let image = ClipboardItem::new(
            origin,
            ClipboardPayload::Image {
                format: ImageFormat::Png,
                bytes: Bytes::from_static(b"png-bytes"),
                width: 2,
                height: 3,
            },
        );
        let Encoded::Json(dto) = encode(&image, 512).unwrap() else {
            panic!("small image must be json");
        };
        let (decoded, _) = decode(dto, None).unwrap();
        assert_eq!(decoded, image);
        let Encoded::Multipart {
            item,
            image: bytes,
            mime,
        } = encode(&image, 4).unwrap()
        else {
            panic!("large image must be multipart");
        };
        assert_eq!(mime, "image/png");
        let (decoded, _) = decode(item, Some(bytes)).unwrap();
        assert_eq!(decoded, image);
    }

    #[test]
    fn rejects_tampered_content() {
        let item = ClipboardItem::new(
            Fingerprint::parse("AB"),
            ClipboardPayload::Text {
                plain: "hello".into(),
                html: None,
                rtf: None,
            },
        );
        let Encoded::Json(mut dto) = encode(&item, 512).unwrap() else {
            panic!()
        };
        dto.text = Some(TextDto {
            plain: "tampered".into(),
            html: None,
            rtf: None,
        });
        assert!(matches!(decode(dto, None), Err(WireError::Invalid(_))));
    }
}
