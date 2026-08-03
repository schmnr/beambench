import { invoke } from '@tauri-apps/api/core';
import type { NodeClipboardCopy } from '../types/vector';

const NODE_CLIPBOARD_ID = 'beambench-node-clipboard';
const NODE_CLIPBOARD_KIND = 'beambench-node-path';

interface NodeClipboardPayload {
  kind: typeof NODE_CLIPBOARD_KIND;
  version: 1;
  pathJson: string;
}

function encodePayload(payload: NodeClipboardPayload): string {
  return encodeURIComponent(JSON.stringify(payload));
}

function decodePayload(value: string): NodeClipboardPayload | null {
  try {
    const parsed = JSON.parse(decodeURIComponent(value)) as Partial<NodeClipboardPayload>;
    if (
      parsed.kind !== NODE_CLIPBOARD_KIND
      || parsed.version !== 1
      || typeof parsed.pathJson !== 'string'
    ) return null;
    return parsed as NodeClipboardPayload;
  } catch {
    return null;
  }
}

export function nodeCopyAsSvg(copy: NodeClipboardCopy): string {
  const width = Math.max(copy.bounds.max.x - copy.bounds.min.x, 0.001);
  const height = Math.max(copy.bounds.max.y - copy.bounds.min.y, 0.001);
  const payload = encodePayload({
    kind: NODE_CLIPBOARD_KIND,
    version: 1,
    pathJson: copy.pathJson,
  });
  return [
    '<svg xmlns="http://www.w3.org/2000/svg"',
    ` viewBox="${copy.bounds.min.x} ${copy.bounds.min.y} ${width} ${height}"`,
    ` width="${width}mm" height="${height}mm">`,
    `<metadata id="${NODE_CLIPBOARD_ID}">${payload}</metadata>`,
    `<path d="${copy.pathData}" fill="none" stroke="#000000"/>`,
    '</svg>',
  ].join('');
}

export function parseNodeClipboardSvg(text: string): string | null {
  if (!text.includes(`<metadata id="${NODE_CLIPBOARD_ID}"`)) return null;
  const match = text.match(
    new RegExp(`<metadata\\s+id=["']${NODE_CLIPBOARD_ID}["'][^>]*>([^<]+)</metadata>`, 'i'),
  );
  if (!match) return null;
  return decodePayload(match[1].trim())?.pathJson ?? null;
}

export async function writeNodeClipboard(copy: NodeClipboardCopy): Promise<void> {
  const svg = nodeCopyAsSvg(copy);
  try {
    await invoke('write_clipboard_text', { text: svg });
  } catch (nativeError) {
    if (!navigator.clipboard?.writeText) throw nativeError;
    await navigator.clipboard.writeText(svg);
  }
}

export async function readNodeClipboard(): Promise<string | null> {
  let text: string | null = null;
  try {
    text = await invoke<string | null>('read_clipboard_text');
  } catch {
    if (navigator.clipboard?.readText) {
      try {
        text = await navigator.clipboard.readText();
      } catch {
        text = null;
      }
    }
  }
  return text ? parseNodeClipboardSvg(text) : null;
}
