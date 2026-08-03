import { describe, expect, it } from 'vitest';
import { nodeCopyAsSvg, parseNodeClipboardSvg } from '../nodeClipboard';

describe('node clipboard SVG', () => {
  it('round-trips the structured node payload inside portable SVG text', () => {
    const pathJson = JSON.stringify({
      subpaths: [{ commands: [{ type: 'move_to', x: 2, y: 3 }], closed: false }],
    });
    const svg = nodeCopyAsSvg({
      pathJson,
      pathData: 'M 2 3 L 12 8',
      bounds: { min: { x: 2, y: 3 }, max: { x: 12, y: 8 } },
      closed: false,
    });

    expect(svg).toContain('<path d="M 2 3 L 12 8"');
    expect(parseNodeClipboardSvg(svg)).toBe(pathJson);
  });

  it('ignores ordinary SVG without Beam Bench node metadata', () => {
    expect(parseNodeClipboardSvg('<svg><path d="M0 0L1 1"/></svg>')).toBeNull();
  });
});
