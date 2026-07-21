import { beforeEach, describe, expect, it, vi } from 'vitest';
import { CanvasRenderer } from '../CanvasRenderer';

const drawOrder = vi.hoisted(() => [] as string[]);

vi.mock('../drawWorkspace', () => ({
  drawBed: vi.fn(() => drawOrder.push('bed')),
  drawGrid: vi.fn(() => drawOrder.push('grid')),
  drawOrigin: vi.fn(() => drawOrder.push('origin')),
  drawRulers: vi.fn(() => drawOrder.push('rulers')),
}));

class MockImage {
  onload: (() => void) | null = null;
  onerror: (() => void) | null = null;
  crossOrigin: string | null = null;
  complete = false;
  naturalWidth = 0;
  static instances: MockImage[] = [];

  constructor() {
    MockImage.instances.push(this);
  }

  set src(_value: string) {
    this.complete = true;
    this.naturalWidth = 1;
    this.onload?.();
  }
}

function mockContext(): CanvasRenderingContext2D {
  const target: Record<string, unknown> = {
    canvas: {},
    globalAlpha: 1,
    imageSmoothingEnabled: true,
  };
  const stack: Array<{ globalAlpha: number }> = [];
  return new Proxy(target, {
    get(obj, prop) {
      if (prop in obj) {
        return obj[prop as string];
      }
      if (prop === 'save') {
        return vi.fn(() => {
          stack.push({ globalAlpha: obj.globalAlpha as number });
        });
      }
      if (prop === 'restore') {
        return vi.fn(() => {
          const previous = stack.pop();
          if (previous) {
            obj.globalAlpha = previous.globalAlpha;
          }
        });
      }
      if (prop === 'drawImage') {
        return vi.fn(() => {
          obj.__alphaAtDraw = obj.globalAlpha;
          drawOrder.push('cameraOverlay');
        });
      }
      if (prop === 'strokeRect') {
        return vi.fn(() => drawOrder.push('cameraAdjustHandle'));
      }
      return vi.fn();
    },
    set(obj, prop, value) {
      obj[prop as string] = value;
      return true;
    },
  }) as unknown as CanvasRenderingContext2D;
}

describe('CanvasRenderer camera overlay', () => {
  beforeEach(() => {
    drawOrder.length = 0;
    MockImage.instances = [];
    vi.stubGlobal('Image', MockImage);
  });

  it('loads camera frames with anonymous CORS so canvas export remains readable', () => {
    const ctx = mockContext();
    const renderer = new CanvasRenderer(ctx);

    renderer.ensureCameraOverlayImage('frame-1', 'asset://frame-1.png');

    expect(MockImage.instances[0]?.crossOrigin).toBe('anonymous');
  });

  it('draws the camera still between grid and origin using configured opacity', () => {
    const ctx = mockContext();
    const renderer = new CanvasRenderer(ctx);
    renderer.ensureCameraOverlayImage('frame-1', 'asset://frame-1.png');

    renderer.renderBaseScene({
      workspace: { bed_width_mm: 300, bed_height_mm: 200, origin: 'top_left' },
      objects: [],
      layers: [],
      selectedObjectIds: [],
      vp: {
        offset: { x: 0, y: 0 },
        zoom: 100,
        canvasWidth: 800,
        canvasHeight: 600,
      },
      gridVisible: true,
      gridSpacingMm: 10,
      toolOverlay: { type: 'none' },
      cameraOverlay: {
        frameHandleId: 'frame-1',
        widthPx: 100,
        heightPx: 80,
        transform: {
          scale: 0.5,
          rotation_deg: 0,
          translation_x: 20,
          translation_y: 30,
        },
        opacity: 0.35,
      },
    });

    expect(drawOrder).toEqual(['bed', 'grid', 'cameraOverlay', 'origin', 'rulers']);
    expect((ctx as unknown as { __alphaAtDraw: number }).__alphaAtDraw).toBeCloseTo(0.35);
    expect(ctx.globalAlpha).toBe(1);
  });

  it('draws camera adjustment handles as tool overlay content', () => {
    const ctx = mockContext();
    const renderer = new CanvasRenderer(ctx);

    renderer.renderToolOverlay({
      workspace: { bed_width_mm: 300, bed_height_mm: 200, origin: 'top_left' },
      objects: [],
      layers: [],
      selectedObjectIds: [],
      vp: {
        offset: { x: 0, y: 0 },
        zoom: 100,
        canvasWidth: 800,
        canvasHeight: 600,
      },
      gridVisible: true,
      gridSpacingMm: 10,
      toolOverlay: {
        type: 'camera-overlay-adjust',
        widthPx: 100,
        heightPx: 80,
        transform: {
          scale: 0.5,
          rotation_deg: 0,
          translation_x: 20,
          translation_y: 30,
        },
      },
    });

    expect(drawOrder.filter((item) => item === 'cameraAdjustHandle')).toHaveLength(4);
  });
});
