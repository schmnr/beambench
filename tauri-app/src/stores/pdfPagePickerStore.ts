import { create } from 'zustand';

export interface PdfPageRequest {
  id: number;
  filename: string;
  pageCount: number;
  finish: (page: number | null) => void;
}

export const usePdfPagePickerStore = create<{ request: PdfPageRequest | null }>(() => ({
  request: null,
}));

let nextRequestId = 0;

export function requestPdfPage(filename: string, pageCount: number): Promise<number | null> {
  if (usePdfPagePickerStore.getState().request) return Promise.resolve(null);
  return new Promise((resolve) => {
    const request: PdfPageRequest = {
      id: ++nextRequestId,
      filename,
      pageCount,
      finish(page) {
        if (usePdfPagePickerStore.getState().request !== request) return;
        usePdfPagePickerStore.setState({ request: null });
        resolve(page);
      },
    };
    usePdfPagePickerStore.setState({ request });
  });
}
