import { invoke } from '@tauri-apps/api/core';

export type PrintMode = 'black' | 'color';

interface PrintDocumentResponse {
  title: string;
  svg: string;
}

const PRINT_ROOT_ID = 'beam-bench-print-root';
const PRINT_STYLE_ID = 'beam-bench-print-style';

const PRINT_CSS = `
  #${PRINT_ROOT_ID} {
    position: fixed;
    left: 0;
    top: 0;
    width: 1px;
    height: 1px;
    overflow: hidden;
    opacity: 0;
    pointer-events: none;
  }

  @media print {
    @page { margin: 0.5in; }
    html, body {
      margin: 0;
      width: 100%;
      height: 100%;
      background: white;
    }
    body > :not(#${PRINT_ROOT_ID}) {
      display: none !important;
    }
    #${PRINT_ROOT_ID} {
      display: flex;
      align-items: center;
      justify-content: center;
      position: static;
      width: 100%;
      height: 100%;
      min-height: 100vh;
      overflow: visible;
      opacity: 1;
      pointer-events: auto;
    }
    #${PRINT_ROOT_ID} svg {
      display: block;
      width: auto;
      height: auto;
      max-width: 100%;
      max-height: 100%;
      break-inside: avoid;
    }
  }
`;

function ensurePrintStyle(): HTMLStyleElement {
  const existing = globalThis.document.getElementById(PRINT_STYLE_ID);
  if (existing instanceof HTMLStyleElement) return existing;

  const style = globalThis.document.createElement('style');
  style.id = PRINT_STYLE_ID;
  style.textContent = PRINT_CSS;
  globalThis.document.head.append(style);
  return style;
}

function waitForPrintLayout(): Promise<void> {
  return new Promise((resolve) => {
    const requestFrame = window.requestAnimationFrame
      ?? ((callback: FrameRequestCallback) => window.setTimeout(callback, 0));
    requestFrame(() => {
      requestFrame(() => resolve());
    });
  });
}

function preparePrintDocument(document: PrintDocumentResponse): () => void {
  globalThis.document.getElementById(PRINT_ROOT_ID)?.remove();

  const style = ensurePrintStyle();
  const root = globalThis.document.createElement('div');
  const previousTitle = globalThis.document.title;
  root.id = PRINT_ROOT_ID;
  root.setAttribute('aria-hidden', 'true');
  root.innerHTML = document.svg;
  globalThis.document.body.append(root);
  globalThis.document.title = document.title || 'Beam Bench Print';

  return () => {
    root.remove();
    style.remove();
    globalThis.document.title = previousTitle;
  };
}

async function openPrintDialog(): Promise<void> {
  try {
    await invoke<void>('print_current_webview');
    return;
  } catch (error) {
    if (typeof window.print !== 'function') {
      throw error;
    }
    window.print();
  }
}

function cleanupAfterPrint(cleanup: () => void): () => void {
  let cleaned = false;
  const cleanupOnce = () => {
    if (cleaned) return;
    cleaned = true;
    window.removeEventListener('afterprint', cleanupOnce);
    cleanup();
  };

  window.addEventListener('afterprint', cleanupOnce, { once: true });
  window.setTimeout(cleanupOnce, 5000);
  return cleanupOnce;
}

async function printSvgDocument(document: PrintDocumentResponse): Promise<void> {
  if (typeof window === 'undefined' || typeof globalThis.document === 'undefined') {
    throw new Error('Printing is only available in a browser window');
  }

  const cleanup = preparePrintDocument(document);
  let cleanupNow = cleanup;

  try {
    await waitForPrintLayout();
    cleanupNow = cleanupAfterPrint(cleanup);
    await openPrintDialog();
  } catch (error) {
    cleanupNow();
    throw error;
  }
}

export const printService = {
  async printProject(mode: PrintMode): Promise<void> {
    const document = await invoke<PrintDocumentResponse>('render_print_document', { mode });
    await printSvgDocument(document);
  },
};
