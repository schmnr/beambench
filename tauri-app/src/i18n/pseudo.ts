/**
 * ICU-/markup-safe pseudo-locale transform for en-XA.
 * Accents literal text, preserves {placeholders} and <tag> markup,
 * pads strings to surface layout overflow.
 */

const ACCENT_MAP: Record<string, string> = {
  a: 'ä', b: 'ƀ', c: 'ç', d: 'ð', e: 'é', f: 'ƒ', g: 'ǵ', h: 'ĥ',
  i: 'í', j: 'ĵ', k: 'ǩ', l: 'ĺ', m: 'ḿ', n: 'ñ', o: 'ö', p: 'ṕ',
  q: 'ǫ', r: 'ŕ', s: 'š', t: 'ţ', u: 'ü', v: 'ṽ', w: 'ŵ', x: 'ẋ',
  y: 'ý', z: 'ž',
  A: 'Ä', B: 'Ɓ', C: 'Ç', D: 'Ð', E: 'É', F: 'Ƒ', G: 'Ǵ', H: 'Ĥ',
  I: 'Í', J: 'Ĵ', K: 'Ǩ', L: 'Ĺ', M: 'Ḿ', N: 'Ñ', O: 'Ö', P: 'Ṕ',
  Q: 'Ǫ', R: 'Ŕ', S: 'Š', T: 'Ţ', U: 'Ü', V: 'Ṽ', W: 'Ŵ', X: 'Ẋ',
  Y: 'Ý', Z: 'Ž',
};

const FILLER = ['one', 'two', 'three', 'four', 'five', 'six', 'seven', 'eight'];

function accentLiteral(s: string): string {
  let out = '';
  for (const ch of s) out += ACCENT_MAP[ch] ?? ch;
  return out;
}

function processSegment(s: string): string {
  let out = '';
  let i = 0;
  let literalBuf = '';

  const flushLiteral = () => {
    if (literalBuf) {
      out += accentLiteral(literalBuf);
      literalBuf = '';
    }
  };

  while (i < s.length) {
    const ch = s[i];

    if (ch === '<') {
      const close = s.indexOf('>', i);
      if (close === -1) {
        literalBuf += ch;
        i += 1;
        continue;
      }
      flushLiteral();
      out += s.slice(i, close + 1);
      i = close + 1;
      continue;
    }

    if (ch === '{') {
      let depth = 1;
      let j = i + 1;
      while (j < s.length && depth > 0) {
        if (s[j] === '{') depth += 1;
        else if (s[j] === '}') depth -= 1;
        if (depth === 0) break;
        j += 1;
      }
      if (depth !== 0) {
        literalBuf += ch;
        i += 1;
        continue;
      }
      flushLiteral();
      const block = s.slice(i, j + 1);
      out += processIcuBlock(block);
      i = j + 1;
      continue;
    }

    literalBuf += ch;
    i += 1;
  }

  flushLiteral();
  return out;
}

function processIcuBlock(block: string): string {
  const inner = block.slice(1, -1);
  if (!/,\s*(plural|select|selectordinal)/.test(inner)) return block;

  let out = '{';
  let i = 0;
  while (i < inner.length) {
    const ch = inner[i];
    if (ch === '{') {
      let depth = 1;
      let j = i + 1;
      while (j < inner.length && depth > 0) {
        if (inner[j] === '{') depth += 1;
        else if (inner[j] === '}') depth -= 1;
        if (depth === 0) break;
        j += 1;
      }
      out += '{' + processSegment(inner.slice(i + 1, j)) + '}';
      i = j + 1;
      continue;
    }
    out += ch;
    i += 1;
  }
  return out + '}';
}

export function transformToPseudo(input: string): string {
  if (input.length === 0) return '⟦⟧';
  const accented = processSegment(input);
  const targetLen = Math.max(Math.floor(input.length * 0.3), 1);
  let pad = '';
  let f = 0;
  while (pad.length < targetLen) {
    pad += ' ' + FILLER[f % FILLER.length];
    f += 1;
  }
  return `⟦${accented}${pad}⟧`;
}

export function buildPseudoBundle(en: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(en)) {
    if (typeof value === 'string') {
      out[key] = transformToPseudo(value);
    } else if (value && typeof value === 'object' && !Array.isArray(value)) {
      out[key] = buildPseudoBundle(value as Record<string, unknown>);
    } else {
      out[key] = value;
    }
  }
  return out;
}
