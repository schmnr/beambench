import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent, waitFor, cleanup } from '@testing-library/react';
import { AdjustImageDialog } from '../AdjustImageDialog';
import { useNotificationStore } from '../../../stores/notificationStore';
import { useProjectStore } from '../../../stores/projectStore';
import { importService } from '../../../services/importService';
import { projectService } from '../../../services/projectService';
import { makeLayer, makeProject, makeProjectObject, makeRasterSettings } from '../../../test-utils/projectFixtures';

vi.mock('../../../services/importService', () => ({
  importService: {
    getImagePresets: vi.fn().mockResolvedValue([]),
    adjustImagePreview: vi.fn().mockResolvedValue({ png_base64: '', width: 1, height: 1 }),
    autoAdjustImage: vi.fn().mockResolvedValue({ brightness: 0, contrast: 0, gamma: 1, sharpen: 0 }),
    saveImagePreset: vi.fn(),
    deleteImagePreset: vi.fn(),
  },
}));

vi.mock('../../../services/projectService', () => ({
  projectService: {
    // dialog now commits through a single atomic backend command.
    applyAdjustImageDialog: vi.fn().mockResolvedValue({
      object: null,
      layer: null,
    }),
  },
}));

const mockedImportService = importService as unknown as Record<string, ReturnType<typeof vi.fn>>;
const mockedProjectService = projectService as unknown as Record<string, ReturnType<typeof vi.fn>>;
const initialProjectState = useProjectStore.getState();
const initialNotificationState = useNotificationStore.getState();

describe('AdjustImageDialog', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    const imageLayer = makeLayer({
      id: 'l1',
      name: 'Image Layer',
      operation: 'image',
      color_tag: '#000000',
      raster_settings: makeRasterSettings({
        dpi: 127,
        mode: 'threshold',
        scan_angle: 0,
        bidirectional: true,
        overscan_mm: 0,
        passes: 1,
        line_interval_mm: 0.2,
        pass_through: false,
        halftone_cells_per_inch: 20,
        halftone_angle_deg: 30,
        newsprint_angle_deg: 60,
        newsprint_frequency: 15,
        invert: true,
        dot_width_correction_mm: 0,
        ramp_length_mm: 0,
      }),
      vector_settings: null,
    });
    useProjectStore.setState({
      project: makeProject({
        metadata: { format_version: '1', app_version: '0.1.0', project_id: 'p1', project_name: 'Test', created_at: '', modified_at: '' },
        workspace: { bed_width_mm: 400, bed_height_mm: 400, origin: 'top_left' },
        layers: [imageLayer],
        objects: [makeProjectObject({
          id: 'img1',
          name: 'Image',
          layer_id: 'l1',
          data: {
            type: 'raster_image',
            asset_key: 'asset-1',
            original_width_px: 100,
            original_height_px: 100,
            adjustments: {
              brightness: 0.2,
              contrast: 0.3,
              gamma: 1.4,
              invert: true,
              threshold: 64,
              saturation: 2,
              sharpen: 1,
              edge_enhance: true,
              enhance_radius: 2,
              enhance_amount: 1.5,
              enhance_denoise: 0.6,
            },
          },
        })],
        assets: [],
      }),
      updateObjectData: vi.fn().mockResolvedValue(undefined),
      loadProject: vi.fn().mockResolvedValue(undefined),
      loadAssetData: vi.fn().mockResolvedValue(null),
    });
  });

  afterEach(() => {
    cleanup();
    useProjectStore.setState(initialProjectState, true);
    useNotificationStore.setState(initialNotificationState, true);
  });

  it('Auto button fills sliders from the backend suggestion', async () => {
    mockedImportService.autoAdjustImage.mockResolvedValue({
      brightness: 0.25, contrast: 0.6, gamma: 1.3, sharpen: 0.2,
    });
    render(<AdjustImageDialog objectId="img1" onClose={vi.fn()} />);

    fireEvent.click(screen.getByTestId('adjust-auto'));

    await waitFor(() => {
      expect(mockedImportService.autoAdjustImage).toHaveBeenCalledWith('img1');
      expect((screen.getByTestId('adjust-brightness') as HTMLInputElement).value).toBe('0.25');
    });
    expect((screen.getByTestId('adjust-contrast') as HTMLInputElement).value).toBe('0.6');
    expect((screen.getByTestId('adjust-gamma') as HTMLInputElement).value).toBe('1.3');
    expect((screen.getByTestId('adjust-sharpen') as HTMLInputElement).value).toBe('0.2');
  });

  it('Reset All clears hidden image adjustments before save', async () => {
    const onClose = vi.fn();
    render(<AdjustImageDialog objectId="img1" onClose={onClose} />);

    fireEvent.click(screen.getByText('Reset All'));
    fireEvent.click(screen.getByRole('button', { name: 'OK' }));

    await waitFor(() => {
      expect(mockedProjectService.applyAdjustImageDialog).toHaveBeenCalled();
    });

    expect(mockedImportService.getImagePresets).toHaveBeenCalled();
    // one atomic call carrying both the reset adjustments and the
    // reset raster settings — no more separate updateObjectData/updateLayerFull.
    expect(mockedProjectService.applyAdjustImageDialog).toHaveBeenCalledWith(
      'img1',
      expect.objectContaining({
        brightness: 0,
        contrast: 0,
        gamma: 1,
        invert: false,
        threshold: 128,
        saturation: 1,
        sharpen: 0,
        edge_enhance: false,
        enhance_radius: 0,
        enhance_amount: 0,
        enhance_denoise: 0,
      }),
      'l1',
      expect.objectContaining({
        mode: 'grayscale',
        invert: false,
        line_interval_mm: 0.1,
        halftone_cells_per_inch: 10,
        halftone_angle_deg: 0,
        newsprint_angle_deg: 45,
        newsprint_frequency: 10,
      }),
    );
    expect(useProjectStore.getState().loadProject).toHaveBeenCalledWith({ invalidatePreview: true });
    expect(onClose).toHaveBeenCalled();
  });

  it('shows threshold and denoise controls and sends updated preview params', async () => {
    render(<AdjustImageDialog objectId="img1" onClose={vi.fn()} />);

    fireEvent.change(screen.getByTestId('adjust-threshold'), { target: { value: '96' } });
    fireEvent.change(screen.getByTestId('adjust-enhance-denoise'), { target: { value: '1.1' } });

    await waitFor(() => {
      expect(mockedImportService.adjustImagePreview).toHaveBeenCalledWith(
        expect.objectContaining({
          threshold: 96,
          enhanceDenoise: 1.1,
        }),
      );
    });
  });

  it('closes when its source object disappears', async () => {
    const onClose = vi.fn();
    render(<AdjustImageDialog objectId="img1" onClose={onClose} />);

    useProjectStore.setState({
      project: {
        ...useProjectStore.getState().project!,
        objects: [],
      },
    });

    await waitFor(() => {
      expect(onClose).toHaveBeenCalled();
    });
  });

  it('saves and reapplies presets with the full raster adjustment payload', async () => {
    mockedImportService.getImagePresets
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([{
        name: 'Preset A',
        adjustments: {
          brightness: 0.25,
          contrast: 0.35,
          gamma: 1.6,
          invert: true,
          threshold: 96,
          saturation: 1.4,
          sharpen: 0.75,
          edge_enhance: true,
          enhance_radius: 1.5,
          enhance_amount: 2.1,
          enhance_denoise: 1.1,
        },
      }]);

    render(<AdjustImageDialog objectId="img1" onClose={vi.fn()} />);

    fireEvent.change(screen.getByTestId('adjust-brightness'), { target: { value: '0.25' } });
    fireEvent.change(screen.getByTestId('adjust-contrast'), { target: { value: '0.35' } });
    fireEvent.change(screen.getByTestId('adjust-gamma'), { target: { value: '1.6' } });
    fireEvent.change(screen.getByTestId('adjust-threshold'), { target: { value: '96' } });
    fireEvent.change(screen.getByTestId('adjust-saturation'), { target: { value: '1.4' } });
    fireEvent.change(screen.getByTestId('adjust-sharpen'), { target: { value: '0.75' } });
    fireEvent.change(screen.getByTestId('adjust-enhance-radius'), { target: { value: '1.5' } });
    fireEvent.change(screen.getByTestId('adjust-enhance-amount'), { target: { value: '2.1' } });
    fireEvent.change(screen.getByTestId('adjust-enhance-denoise'), { target: { value: '1.1' } });
    fireEvent.click(screen.getByText('Save'));
    fireEvent.change(screen.getByPlaceholderText('Name'), { target: { value: 'Preset A' } });
    fireEvent.click(screen.getByTestId('adjust-save-preset-confirm'));

    await waitFor(() => {
      expect(mockedImportService.saveImagePreset).toHaveBeenCalledWith(
        'Preset A',
        expect.objectContaining({
          brightness: 0.25,
          contrast: 0.35,
          gamma: 1.6,
          invert: true,
          threshold: 96,
          saturation: 1.4,
          sharpen: 0.75,
          edge_enhance: true,
          enhance_radius: 1.5,
          enhance_amount: 2.1,
          enhance_denoise: 1.1,
        }),
      );
    });

    fireEvent.change(screen.getByTestId('adjust-threshold'), { target: { value: '120' } });
    fireEvent.change(screen.getByTestId('adjust-enhance-denoise'), { target: { value: '0.2' } });
    fireEvent.change(screen.getByTestId('adjust-saturation'), { target: { value: '1' } });
    fireEvent.change(screen.getByTestId('adjust-sharpen'), { target: { value: '0' } });
    fireEvent.click(
      ((screen.getByText('Invert').nextElementSibling as HTMLElement).querySelector(
        'input'
      ) as HTMLInputElement)
    );
    fireEvent.click(
      ((screen.getByText('Edge Enhance').nextElementSibling as HTMLElement).querySelector(
        'input'
      ) as HTMLInputElement)
    );

    fireEvent.change(screen.getByTestId('adjust-preset-select'), { target: { value: 'Preset A' } });

    await waitFor(() => {
      expect(Number((screen.getByTestId('adjust-threshold') as HTMLInputElement).value)).toBe(96);
      expect(Number((screen.getByTestId('adjust-enhance-denoise') as HTMLInputElement).value)).toBe(1.1);
      expect(Number((screen.getByTestId('adjust-saturation') as HTMLInputElement).value)).toBe(1.4);
      expect(Number((screen.getByTestId('adjust-sharpen') as HTMLInputElement).value)).toBe(0.75);
      expect(
        ((screen.getByText('Invert').nextElementSibling as HTMLElement).querySelector(
          'input'
        ) as HTMLInputElement).checked
      ).toBe(true);
      expect(
        ((screen.getByText('Edge Enhance').nextElementSibling as HTMLElement).querySelector(
          'input'
        ) as HTMLInputElement).checked
      ).toBe(true);
    });
  });

  it('shows an error notification when saving a preset fails from the Enter flow', async () => {
    const push = vi.fn();
    useNotificationStore.setState({ push });
    mockedImportService.saveImagePreset.mockRejectedValueOnce(new Error('save failed'));

    render(<AdjustImageDialog objectId="img1" onClose={vi.fn()} />);

    fireEvent.click(screen.getByText('Save'));
    fireEvent.change(screen.getByPlaceholderText('Name'), { target: { value: 'Preset A' } });
    fireEvent.keyDown(screen.getByPlaceholderText('Name'), { key: 'Enter' });

    await waitFor(() => {
      expect(push).toHaveBeenCalledWith('Operation failed: Error: save failed', 'error');
      expect(screen.getByPlaceholderText('Name')).toBeDefined();
    });
  });

  it('shows an error notification when saving a preset fails from the button flow', async () => {
    const push = vi.fn();
    useNotificationStore.setState({ push });
    mockedImportService.saveImagePreset.mockRejectedValueOnce(new Error('save failed'));

    render(<AdjustImageDialog objectId="img1" onClose={vi.fn()} />);

    fireEvent.click(screen.getByText('Save'));
    fireEvent.change(screen.getByPlaceholderText('Name'), { target: { value: 'Preset A' } });
    fireEvent.click(screen.getByTestId('adjust-save-preset-confirm'));

    await waitFor(() => {
      expect(push).toHaveBeenCalledWith('Operation failed: Error: save failed', 'error');
      expect(screen.getByPlaceholderText('Name')).toBeDefined();
    });
  });

  it('shows an error notification when deleting a preset fails', async () => {
    const push = vi.fn();
    useNotificationStore.setState({ push });
    mockedImportService.getImagePresets.mockResolvedValueOnce([{
      name: 'Preset A',
      adjustments: {
        brightness: 0.25,
        contrast: 0.35,
        gamma: 1.6,
        invert: true,
        threshold: 96,
        saturation: 1.4,
        sharpen: 0.75,
        edge_enhance: true,
        enhance_radius: 1.5,
        enhance_amount: 2.1,
        enhance_denoise: 1.1,
      },
    }]);
    mockedImportService.deleteImagePreset.mockRejectedValueOnce(new Error('delete failed'));

    render(<AdjustImageDialog objectId="img1" onClose={vi.fn()} />);

    await waitFor(() => {
      expect(screen.getByTestId('adjust-preset-select')).toBeDefined();
    });
    fireEvent.change(screen.getByTestId('adjust-preset-select'), { target: { value: 'Preset A' } });
    fireEvent.click(screen.getByText('Delete'));

    await waitFor(() => {
      expect(push).toHaveBeenCalledWith('Operation failed: Error: delete failed', 'error');
      expect((screen.getByTestId('adjust-preset-select') as HTMLSelectElement).value).toBe('Preset A');
    });
  });
});
