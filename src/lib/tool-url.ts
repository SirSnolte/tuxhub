// The address of a file of a tool's folder, as tuxhub serves it. Linux uses
// the scheme `tool://<id>/`; the web view of Windows serves custom schemes
// as `http://<scheme>.localhost/`, so the tool goes into the path there.
import type { Platform } from "./api";

export function toolUrl(os: Platform, id: string, path: string): string {
  const file = path.replace(/^\/+/, "");
  return os === "windows" ? `http://tool.localhost/${id}/${file}` : `tool://${id}/${file}`;
}
