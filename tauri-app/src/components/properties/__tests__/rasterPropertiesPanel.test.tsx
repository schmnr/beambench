import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, cleanup, fireEvent } from '@testing-library/react';
import { RasterPropertiesPanel } from '../RasterPropertiesPanel';
import { useProjectStore } from '../../../stores/projectStore';
import { makeLayer, makeProject, makeProjectObject } from '../../../test-utils/projectFixtures';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(null) }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn().mockReturnValue(new Promise(() => {})) }));

const initialState = useProjectStore.getState();

afterEach(() => {
  cleanup();
  useProjectStore.setState(initialState, true);
});

describe('RasterPropertiesPanel', () => {
  it('renders sharpen and edge enhance controls', () => {
    const data = {
      type: 'raster_image' as const,
      asset_key: 'img_1',
      original_width_px: 100,
      original_height_px: 100,
      adjustments: {
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
      },
    };

    render(<RasterPropertiesPanel objectId="obj1" data={data} />);
    expect(screen.getByText('Sharpen')).toBeDefined();
    expect(screen.getByText('Edge Enhance')).toBeDefined();
  });

  it('renders masks and calls polarity/remove actions', () => {
    const setImageMaskPolarity = vi.fn().mockResolvedValue(undefined);
    const removeImageMask = vi.fn().mockResolvedValue(undefined);
    const data = {
      type: 'raster_image' as const,
      asset_key: 'img_1',
      original_width_px: 100,
      original_height_px: 100,
      masks: [{ object_id: 'mask1', polarity: 'keep_inside' as const }],
    };
    useProjectStore.setState({
      setImageMaskPolarity,
      removeImageMask,
      project: makeProject({
        layers: [makeLayer({ id: 'l1', name: 'Layer 1' })],
        objects: [
          makeProjectObject({ id: 'obj1', name: 'Image', layer_id: 'l1', data }),
          makeProjectObject({
            id: 'mask1',
            name: 'Mask Rect',
            layer_id: 'l1',
            data: { type: 'shape', kind: 'rectangle', width: 10, height: 10, corner_radius: 0 },
          }),
        ],
      }),
    });

    render(<RasterPropertiesPanel objectId="obj1" data={data} />);

    expect(screen.getByText('Masks: 1')).toBeDefined();
    expect(screen.getByText('Mask Rect')).toBeDefined();
    fireEvent.change(screen.getByDisplayValue('Keep Inside'), { target: { value: 'keep_outside' } });
    expect(setImageMaskPolarity).toHaveBeenCalledWith('obj1', 'mask1', 'keep_outside');

    fireEvent.click(screen.getByText('Remove'));
    expect(removeImageMask).toHaveBeenCalledWith('obj1', 'mask1');
    fireEvent.click(screen.getByText('Clear'));
    expect(removeImageMask).toHaveBeenCalledWith('obj1');
  });
});
