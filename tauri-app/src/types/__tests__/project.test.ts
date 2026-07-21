import { describe, it, expect } from 'vitest';
import type {
  ObjectData,
  OperationType,
  StartFromMode,
  AnchorPoint,
  BarcodeType,
  OffsetFillGroupingMode,
} from '../project';

describe('types/project extensions', () => {
  it('ObjectData polygon variant has correct shape', () => {
    const polygon: ObjectData = {
      type: 'polygon',
      sides: 6,
      radius: 25,
    };
    expect(polygon.type).toBe('polygon');
    if (polygon.type === 'polygon') {
      expect(polygon.sides).toBe(6);
    }
  });

  it('ObjectData barcode variant has correct shape', () => {
    const barcode: ObjectData = {
      type: 'barcode',
      barcode_type: 'qr_code',
      data: 'https://example.com',
      width: 30,
      height: 30,
    };
    expect(barcode.type).toBe('barcode');
    expect(barcode.barcode_type).toBe('qr_code');
  });

  it('BarcodeType accepts only backend-compatible literals', () => {
    const barcodeTypes: BarcodeType[] = [
      'code128',
      'code39',
      'code93',
      'codabar',
      'standard_2_of_5',
      'ean13',
      'ean8',
      'upc_a',
      'qr_code',
      'data_matrix',
      'pdf417',
    ];
    expect(barcodeTypes).toHaveLength(11);
  });

  it('StartFromMode and AnchorPoint accept all literals', () => {
    const modes: StartFromMode[] = ['absolute_coords', 'user_origin', 'current_position'];
    expect(modes).toHaveLength(3);

    const anchors: AnchorPoint[] = [
      'top_left', 'top_center', 'top_right',
      'center_left', 'center', 'center_right',
      'bottom_left', 'bottom_center', 'bottom_right',
    ];
    expect(anchors).toHaveLength(9);
  });

  it('offset_fill is assignable to OperationType', () => {
    const op: OperationType = 'offset_fill';
    expect(op).toBe('offset_fill');
  });

  it('OffsetFillGroupingMode accepts backend-compatible literals', () => {
    const modes: OffsetFillGroupingMode[] = [
      'all_shapes_at_once',
      'groups_together',
      'shapes_individually',
    ];
    expect(modes).toHaveLength(3);
  });
});
