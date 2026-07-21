import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useVariableTextStore } from '../variableTextStore';

vi.mock('../../services/variableTextService', () => ({
  variableTextService: {
    parseMergeFields: vi.fn(),
    loadCsvFile: vi.fn(),
    resolveVariableText: vi.fn(),
    generateBatchPreview: vi.fn(),
    generateVariableTextBatch: vi.fn(),
  },
}));

import { variableTextService } from '../../services/variableTextService';

const mockedService = variableTextService as {
  parseMergeFields: ReturnType<typeof vi.fn>;
  loadCsvFile: ReturnType<typeof vi.fn>;
  resolveVariableText: ReturnType<typeof vi.fn>;
  generateBatchPreview: ReturnType<typeof vi.fn>;
  generateVariableTextBatch: ReturnType<typeof vi.fn>;
};

beforeEach(() => {
  useVariableTextStore.setState({
    template: null,
    source: null,
    mode: null,
    offset: 0,
    mergeFields: [],
    warnings: [],
    previewRow: 1,
    previewText: null,
    jobStartTime: null,
  });
  vi.clearAllMocks();
});

describe('variableTextStore', () => {
  it('starts empty', () => {
    const state = useVariableTextStore.getState();
    expect(state.template).toBeNull();
    expect(state.source).toBeNull();
    expect(state.mode).toBeNull();
    expect(state.offset).toBe(0);
    expect(state.mergeFields).toEqual([]);
    expect(state.warnings).toEqual([]);
    expect(state.previewText).toBeNull();
  });

  it('auto-initializes a source for variable templates', () => {
    useVariableTextStore.getState().setTemplate('SN-{Serial}');
    const state = useVariableTextStore.getState();
    expect(state.template).toBe('SN-{Serial}');
    expect(state.source).not.toBeNull();
    expect(state.source?.current).toBe(1);
    expect(state.source?.start).toBe(1);
    expect(state.source?.end).toBe(1);
    expect(state.warnings).toContain(
      'Template contains {Serial} placeholders outside Serial Number mode.',
    );
  });

  it('loadCsv populates csv rows and sequence range', async () => {
    mockedService.loadCsvFile.mockResolvedValue({
      headers: ['Name', 'City'],
      rows: [['Alice', 'Boston'], ['Bob', 'Denver']],
    });

    await useVariableTextStore.getState().loadCsv('/tmp/test.csv');

    const state = useVariableTextStore.getState();
    expect(state.source).not.toBeNull();
    expect(state.source?.csvPath).toBe('/tmp/test.csv');
    expect(state.source?.csvData).toEqual([
      ['Name', 'City'],
      ['Alice', 'Boston'],
      ['Bob', 'Denver'],
    ]);
    expect(state.source?.current).toBe(0);
    expect(state.source?.start).toBe(0);
    expect(state.source?.end).toBe(1);
    expect(state.previewRow).toBe(0);
  });

  it('clearCsv keeps the source but resets sequence state', () => {
    useVariableTextStore.setState({
      source: {
        csvPath: '/tmp/test.csv',
        csvData: [['Name'], ['Alice']],
        fieldDefaults: {},
        current: 0,
        currentRow: 0,
        start: 0,
        end: 0,
        advanceBy: 1,
        autoAdvance: false,
        totalCopies: 2,
      },
      previewText: 'Alice',
      previewRow: 0,
    });

    useVariableTextStore.getState().clearCsv();

    const state = useVariableTextStore.getState();
    expect(state.source?.csvPath).toBeNull();
    expect(state.source?.csvData).toEqual([]);
    expect(state.source?.current).toBe(1);
    expect(state.source?.start).toBe(1);
    expect(state.source?.end).toBe(1);
    expect(state.previewText).toBeNull();
  });

  it('refreshPreview resolves using the current sequence row', async () => {
    useVariableTextStore.setState({
      template: 'Hello {CSV:Name}',
      mode: 'merge_csv',
      source: {
        csvPath: '/tmp/test.csv',
        csvData: [['Name'], ['Alice'], ['Bob']],
        fieldDefaults: {},
        current: 1,
        currentRow: 1,
        start: 0,
        end: 1,
        advanceBy: 1,
        autoAdvance: false,
        totalCopies: 2,
      },
    });
    mockedService.resolveVariableText.mockResolvedValue('Hello Bob');

    await useVariableTextStore.getState().refreshPreview('obj-1');

    expect(mockedService.resolveVariableText).toHaveBeenCalledWith(
      'obj-1',
      expect.objectContaining({
        template: 'Hello {CSV:Name}',
        mode: 'merge_csv',
        source: expect.objectContaining({ current: 1 }),
      }),
      1,
    );
    expect(useVariableTextStore.getState().previewText).toBe('Hello Bob');
  });

  it('next, previous, and reset use inclusive wrap math', () => {
    useVariableTextStore.setState({
      source: {
        csvPath: null,
        csvData: [],
        fieldDefaults: {},
        current: 12,
        currentRow: 12,
        start: 10,
        end: 15,
        advanceBy: 4,
        autoAdvance: false,
        totalCopies: 1,
      },
      previewRow: 12,
    });

    useVariableTextStore.getState().next();
    expect(useVariableTextStore.getState().source?.current).toBe(10);

    useVariableTextStore.getState().previous();
    expect(useVariableTextStore.getState().source?.current).toBe(12);

    useVariableTextStore.getState().reset();
    expect(useVariableTextStore.getState().source?.current).toBe(10);
  });

  it('setters create and update explicit sequence fields', () => {
    useVariableTextStore.getState().setSerialStart(100);
    useVariableTextStore.getState().setAdvanceBy(5);
    useVariableTextStore.getState().setEnd(140);
    useVariableTextStore.getState().setAutoAdvance(true);
    useVariableTextStore.getState().setOffset(-2);
    useVariableTextStore.getState().setMode('serial_number');

    const state = useVariableTextStore.getState();
    expect(state.source?.start).toBe(100);
    expect(state.source?.current).toBe(100);
    expect(state.source?.advanceBy).toBe(5);
    expect(state.source?.end).toBe(140);
    expect(state.source?.autoAdvance).toBe(true);
    expect(state.offset).toBe(-2);
    expect(state.mode).toBe('serial_number');
  });

  it('updates warnings from the mode mismatch matrix', () => {
    useVariableTextStore.getState().setTemplate('{Cut:Speed} {Const:Name}');
    useVariableTextStore.getState().setMode('serial_number');
    expect(useVariableTextStore.getState().warnings).toContain(
      'Template contains {Cut:...} placeholders outside Cut Setting mode.',
    );

    useVariableTextStore.getState().setMode('cut_setting');
    expect(useVariableTextStore.getState().warnings).toEqual([]);
  });

  it('captureJobStartTime and clearJobStartTime update the timestamp', () => {
    useVariableTextStore.getState().captureJobStartTime();
    const time = useVariableTextStore.getState().jobStartTime;
    expect(time).not.toBeNull();
    expect(Number.isNaN(new Date(time as string).getTime())).toBe(false);

    useVariableTextStore.getState().clearJobStartTime();
    expect(useVariableTextStore.getState().jobStartTime).toBeNull();
  });
});
