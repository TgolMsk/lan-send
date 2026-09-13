//! Language of the native pieces (tray menu, file dialogs). The webview
//! has its own tables under `src/locales`; this mirrors the same eight
//! locales and the same `system` resolution so both sides agree.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Locale {
    #[serde(rename = "en")]
    En,
    #[serde(rename = "zh-Hans")]
    ZhHans,
    #[serde(rename = "zh-Hant")]
    ZhHant,
    #[serde(rename = "ja")]
    Ja,
    #[serde(rename = "ko")]
    Ko,
    #[serde(rename = "de")]
    De,
    #[serde(rename = "fr")]
    Fr,
    #[serde(rename = "es")]
    Es,
}

impl Locale {
    pub const ALL: [Locale; 8] = [
        Locale::En,
        Locale::ZhHans,
        Locale::ZhHant,
        Locale::Ja,
        Locale::Ko,
        Locale::De,
        Locale::Fr,
        Locale::Es,
    ];

    pub fn tag(self) -> &'static str {
        match self {
            Locale::En => "en",
            Locale::ZhHans => "zh-Hans",
            Locale::ZhHant => "zh-Hant",
            Locale::Ja => "ja",
            Locale::Ko => "ko",
            Locale::De => "de",
            Locale::Fr => "fr",
            Locale::Es => "es",
        }
    }

    /// The locale a BCP 47 tag maps to, if we ship it (`zh-TW` → `zh-Hant`,
    /// `de-AT` → `de`, …).
    pub fn for_tag(tag: &str) -> Option<Locale> {
        let lower = tag.to_ascii_lowercase();
        if let Some(rest) = lower.strip_prefix("zh") {
            let traditional = ["hant", "tw", "hk", "mo"]
                .iter()
                .any(|marker| rest.contains(marker));
            return Some(if traditional {
                Locale::ZhHant
            } else {
                Locale::ZhHans
            });
        }
        let base = lower.split(['-', '_']).next().unwrap_or("");
        Locale::ALL.into_iter().find(|locale| locale.tag() == base)
    }

    /// The locale of the operating system, English when unknown.
    pub fn system() -> Locale {
        sys_locale::get_locales()
            .find_map(|tag| Locale::for_tag(&tag))
            .unwrap_or(Locale::En)
    }

    /// Resolves the `app.language` setting (`system` or a tag).
    pub fn from_setting(setting: &str) -> Locale {
        match setting {
            "system" | "" => Locale::system(),
            "zh" => Locale::ZhHans,
            tag => Locale::for_tag(tag).unwrap_or_else(Locale::system),
        }
    }

    pub fn strings(self) -> &'static Strings {
        &TABLES[self as usize]
    }
}

/// The handful of strings the native side shows.
pub struct Strings {
    pub tray_open: &'static str,
    pub tray_push: &'static str,
    pub tray_quit: &'static str,
    pub dialog_files: &'static str,
    pub dialog_folders: &'static str,
    pub dialog_media: &'static str,
    pub dialog_receive_dir: &'static str,
}

static TABLES: [Strings; 8] = [
    Strings {
        tray_open: "Open LanSend",
        tray_push: "Push clipboard to paired devices",
        tray_quit: "Quit",
        dialog_files: "Choose files to send",
        dialog_folders: "Choose folders to send",
        dialog_media: "Choose photos or videos to send",
        dialog_receive_dir: "Choose the receive folder",
    },
    Strings {
        tray_open: "打开 LanSend",
        tray_push: "把剪贴板推送到已配对设备",
        tray_quit: "退出",
        dialog_files: "选择要发送的文件",
        dialog_folders: "选择要发送的文件夹",
        dialog_media: "选择要发送的照片或视频",
        dialog_receive_dir: "选择接收文件夹",
    },
    Strings {
        tray_open: "開啟 LanSend",
        tray_push: "將剪貼簿推送到已配對裝置",
        tray_quit: "結束",
        dialog_files: "選擇要傳送的檔案",
        dialog_folders: "選擇要傳送的資料夾",
        dialog_media: "選擇要傳送的相片或影片",
        dialog_receive_dir: "選擇接收資料夾",
    },
    Strings {
        tray_open: "LanSendを開く",
        tray_push: "クリップボードをペアリング済みデバイスに送信",
        tray_quit: "終了",
        dialog_files: "送信するファイルを選択",
        dialog_folders: "送信するフォルダを選択",
        dialog_media: "送信する写真または動画を選択",
        dialog_receive_dir: "受信フォルダを選択",
    },
    Strings {
        tray_open: "LanSend 열기",
        tray_push: "페어링된 기기로 클립보드 보내기",
        tray_quit: "종료",
        dialog_files: "보낼 파일 선택",
        dialog_folders: "보낼 폴더 선택",
        dialog_media: "보낼 사진 또는 동영상 선택",
        dialog_receive_dir: "받는 폴더 선택",
    },
    Strings {
        tray_open: "LanSend öffnen",
        tray_push: "Zwischenablage an gekoppelte Geräte senden",
        tray_quit: "Beenden",
        dialog_files: "Zu sendende Dateien auswählen",
        dialog_folders: "Zu sendende Ordner auswählen",
        dialog_media: "Zu sendende Fotos oder Videos auswählen",
        dialog_receive_dir: "Empfangsordner auswählen",
    },
    Strings {
        tray_open: "Ouvrir LanSend",
        tray_push: "Envoyer le presse-papiers aux appareils jumelés",
        tray_quit: "Quitter",
        dialog_files: "Choisir les fichiers à envoyer",
        dialog_folders: "Choisir les dossiers à envoyer",
        dialog_media: "Choisir des photos ou vidéos à envoyer",
        dialog_receive_dir: "Choisir le dossier de réception",
    },
    Strings {
        tray_open: "Abrir LanSend",
        tray_push: "Enviar portapapeles a los dispositivos emparejados",
        tray_quit: "Salir",
        dialog_files: "Elige los archivos que quieres enviar",
        dialog_folders: "Elige las carpetas que quieres enviar",
        dialog_media: "Elige las fotos o los vídeos que quieres enviar",
        dialog_receive_dir: "Elige la carpeta de recepción",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_tags() {
        assert_eq!(Locale::for_tag("zh-Hans-CN"), Some(Locale::ZhHans));
        assert_eq!(Locale::for_tag("zh-TW"), Some(Locale::ZhHant));
        assert_eq!(Locale::for_tag("zh_HK"), Some(Locale::ZhHant));
        assert_eq!(Locale::for_tag("de-AT"), Some(Locale::De));
        assert_eq!(Locale::for_tag("pt-BR"), None);
        assert_eq!(Locale::from_setting("ja"), Locale::Ja);
    }

    #[test]
    fn tables_follow_the_enum_order() {
        assert_eq!(Locale::ZhHant as usize, 2);
        assert!(Locale::ZhHant.strings().tray_quit != Locale::En.strings().tray_quit);
    }
}
