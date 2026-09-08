// Inline line icons (24px grid, stroke based) so the bundle stays free of
// icon packages.

import type { SVGProps } from "react";

const paths: Record<string, string> = {
  devices: "M4 6h16v9H4zM2 18h20M9 15v3M15 15v3",
  transfers: "M7 4v14M3 14l4 4 4-4M17 20V6M13 10l4-4 4 4",
  clipboard: "M9 4h6v3H9zM6 6h1v14h10V6h1M9 12h6M9 16h4",
  history: "M4 12a8 8 0 1 0 2.3-5.7M4 4v4h4M12 8v4l3 2",
  settings: "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8zM4.9 12l-1.4-2.4 2-1.9 2.7.8 1.7-1.6-.2-2.8L12 3.4l2.3.7-.2 2.8 1.7 1.6 2.7-.8 2 1.9L19.1 12l1.4 2.4-2 1.9-2.7-.8-1.7 1.6.2 2.8-2.3.7-2.3-.7.2-2.8-1.7-1.6-2.7.8-2-1.9z",
  send: "M4 12l16-8-6 16-2-6z",
  refresh: "M20 12a8 8 0 1 1-2.3-5.7M20 4v4h-4",
  star: "M12 3l2.8 5.9 6.4.8-4.7 4.4 1.2 6.3L12 17.3l-5.7 3.1 1.2-6.3L2.8 9.7l6.4-.8z",
  link: "M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1",
  unlink: "M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1M4 4l16 16",
  pencil: "M4 20l4-1L19 8l-3-3L5 16zM14 7l3 3",
  trash: "M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13M10 11v6M14 11v6",
  x: "M6 6l12 12M18 6L6 18",
  check: "M5 12l5 5 9-10",
  folder: "M3 7h6l2 2h10v10H3z",
  file: "M6 3h8l4 4v14H6zM14 3v4h4",
  audio: "M9 18V6l10-2v12M9 18a2.5 2.5 0 1 1-5 0 2.5 2.5 0 0 1 5 0zM19 16a2.5 2.5 0 1 1-5 0 2.5 2.5 0 0 1 5 0z",
  image: "M4 5h16v14H4zM8 13l3-3 4 4 2-2 3 3M9 9h.01",
  plus: "M12 5v14M5 12h14",
  search: "M11 5a6 6 0 1 0 0 12 6 6 0 0 0 0-12zM20 20l-4.5-4.5",
  copy: "M8 8h12v12H8zM4 16V4h12",
  phone: "M7 3h10v18H7zM11 18h2",
  laptop: "M4 6h16v10H4zM2 19h20",
  desktop: "M3 5h18v11H3zM9 20h6M12 16v4",
  server: "M4 5h16v6H4zM4 13h16v6H4zM8 8h.01M8 16h.01",
  globe: "M12 3a9 9 0 1 0 0 18 9 9 0 0 0 0-18zM3 12h18M12 3c3 3 3 15 0 18M12 3c-3 3-3 15 0 18",
  download: "M12 4v12M7 11l5 5 5-5M4 20h16",
  upload: "M12 20V8M7 13l5-5 5 5M4 4h16",
  more: "M6 12h.01M12 12h.01M18 12h.01",
  chevron: "M9 6l6 6-6 6",
  eye: "M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12zM12 9a3 3 0 1 0 0 6 3 3 0 0 0 0-6z",
  bolt: "M13 3L4 14h7l-1 7 9-11h-7z",
  shield: "M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z",
  bell: "M6 16V11a6 6 0 1 1 12 0v5l2 2H4zM10 21h4",
  text: "M5 6h14M12 6v14M8 20h8",
  pause: "M8 5v14M16 5v14",
  open: "M14 4h6v6M20 4l-9 9M19 14v6H5V6h6",
};

export type IconName = keyof typeof paths;

export function Icon({ name, size = 18, ...rest }: { name: IconName; size?: number } & SVGProps<SVGSVGElement>) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill={name === "star" && rest.fill ? rest.fill : "none"}
      stroke="currentColor"
      strokeWidth={1.8}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
      {...rest}
    >
      <path d={paths[name]} />
    </svg>
  );
}
