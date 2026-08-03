import { useState, useEffect, useRef } from 'react';
import { createPortal } from 'react-dom';
import { useTranslation } from 'react-i18next';
import { useProjectStore } from '../../stores/projectStore';
import { useNotificationStore } from '../../stores/notificationStore';
import { useAppStore } from '../../stores/appStore';
import type { BarcodeType, QrErrorCorrection } from '../../types/project';
import { TextInput } from '../shared/TextInput';
import { NumberInput } from '../shared/NumberInput';
import { Select } from '../shared/Select';
import { mmToDisplay, displayToMm, roundDisplayLength, lengthStep, lengthUnitLabel, labelWithUnit } from '../../utils/lengthUnits';
import { QrCode } from 'lucide-react';
import { MovableResizableDialogFrame } from '../shared/MovableResizableDialogFrame';
import { DIALOG_TONE, DialogButton, DialogFooter, DialogSection } from '../shared/DialogPrimitives';

interface BarcodeDialogProps {
  layerId: string;
  onClose: () => void;
}

const BARCODE_TYPES: BarcodeType[] = [
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

type QrPayloadMode = 'text' | 'wifi' | 'contact';
const QR_PAYLOAD_MODES: QrPayloadMode[] = ['text', 'wifi', 'contact'];

function isOneDimensional(type: BarcodeType): boolean {
  return ['code128', 'code39', 'code93', 'codabar', 'standard_2_of_5', 'ean13', 'ean8', 'upc_a'].includes(type);
}

function escapeQrField(value: string): string {
  return value.replace(/([\\;,:"])/g, '\\$1');
}

export function BarcodeDialog({ layerId, onClose }: BarcodeDialogProps) {
  const { t } = useTranslation();
  const projectId = useProjectStore((s) => s.project?.metadata.project_id ?? null);
  const displayUnit = useAppStore((s) => s.settings?.display_unit) ?? 'mm';
  const [barcodeType, setBarcodeType] = useState<BarcodeType>('qr_code');
  const [data, setData] = useState('');
  const [width, setWidth] = useState(30);
  const [height, setHeight] = useState(30);
  const [showText, setShowText] = useState(true);
  const [qrErrorCorrection, setQrErrorCorrection] = useState<QrErrorCorrection>('medium');
  const [dataMatrixForceSquare, setDataMatrixForceSquare] = useState(false);
  const [qrPayloadMode, setQrPayloadMode] = useState<QrPayloadMode>('text');
  const [wifiSsid, setWifiSsid] = useState('');
  const [wifiPassword, setWifiPassword] = useState('');
  const [wifiSecurity, setWifiSecurity] = useState('WPA');
  const [contactName, setContactName] = useState('');
  const [contactOrg, setContactOrg] = useState('');
  const [contactPhone, setContactPhone] = useState('');
  const [contactEmail, setContactEmail] = useState('');
  const initialProjectIdRef = useRef(projectId);

	  const qrEccOptions: { value: QrErrorCorrection; label: string }[] = [
    { value: 'low', label: t('dialog.barcode.qr_ecc_low') },
    { value: 'medium', label: t('dialog.barcode.qr_ecc_medium') },
    { value: 'quartile', label: t('dialog.barcode.qr_ecc_quartile') },
    { value: 'high', label: t('dialog.barcode.qr_ecc_high') },
	  ];
  const barcodeOptions: { value: BarcodeType; label: string }[] = BARCODE_TYPES.map((value) => ({
    value,
    label: t(`dialog.barcode.type_${value}`),
  }));

  const qrModeLabels: Record<QrPayloadMode, string> = {
    text: t('dialog.barcode.mode_text'),
    wifi: t('dialog.barcode.mode_wifi'),
    contact: t('dialog.barcode.mode_contact'),
  };

  const securityOptions = [
    { value: 'WPA', label: t('dialog.barcode.wifi_security_wpa') },
    { value: 'WEP', label: t('dialog.barcode.wifi_security_wep') },
    { value: 'nopass', label: t('dialog.barcode.wifi_security_none') },
  ];

  useEffect(() => {
    if (projectId !== initialProjectIdRef.current) {
      onClose();
    }
  }, [projectId, onClose]);

  const payload = (() => {
    if (barcodeType !== 'qr_code') return data.trim();
    if (qrPayloadMode === 'wifi') {
      return `WIFI:T:${escapeQrField(wifiSecurity)};S:${escapeQrField(wifiSsid)};P:${escapeQrField(wifiPassword)};;`;
    }
    if (qrPayloadMode === 'contact') {
      return [
        'BEGIN:VCARD',
        'VERSION:3.0',
        contactName ? `FN:${contactName}` : '',
        contactOrg ? `ORG:${contactOrg}` : '',
        contactPhone ? `TEL:${contactPhone}` : '',
        contactEmail ? `EMAIL:${contactEmail}` : '',
        'END:VCARD',
      ].filter(Boolean).join('\n');
    }
    return data.trim();
  })();

  const handleSubmit = async () => {
    if (!payload.trim()) return;
    const barcodeLabel = barcodeOptions.find((option) => option.value === barcodeType)?.label ?? barcodeType;
    const currentProject = useProjectStore.getState().project;
    const currentProjectId = currentProject?.metadata.project_id ?? null;

    if (currentProjectId !== initialProjectIdRef.current) {
      useNotificationStore.getState().push(t('dialog.barcode.error_project_changed'), 'warning');
      onClose();
      return;
    }

    if (currentProject && !currentProject.layers.some((layer) => layer.id === layerId)) {
      useNotificationStore.getState().push(t('dialog.barcode.error_layer_unavailable'), 'warning');
      onClose();
      return;
    }

    const created = await useProjectStore.getState().addObject(
      t('dialog.barcode.label_template', { type: barcodeLabel }),
      layerId,
      {
        type: 'barcode',
        barcode_type: barcodeType,
        data: payload.trim(),
        width,
        height,
        options: {
          show_text: isOneDimensional(barcodeType) ? showText : false,
          qr_error_correction: qrErrorCorrection,
          data_matrix_force_square: barcodeType === 'data_matrix' ? dataMatrixForceSquare : false,
        },
      } as never,
      { min: { x: 0, y: 0 }, max: { x: width, y: height } },
    );
    if (created) {
      onClose();
    }
  };

  return createPortal(
    <MovableResizableDialogFrame
      title={t('dialog.barcode.title')}
      titleId="barcode-dialog-title"
      testId="barcode-dialog"
      initialWidth={460}
      initialHeight={520}
      minWidth={400}
      minHeight={400}
      onRequestClose={onClose}
      closeOnBackdropClick
      footer={(
        <DialogFooter>
          <DialogButton tone={DIALOG_TONE.quiet} onClick={onClose}>{t('common.cancel')}</DialogButton>
          <DialogButton tone={DIALOG_TONE.primary} data-testid="barcode-submit" onClick={() => void handleSubmit()} disabled={!payload.trim()}>
            {t('dialog.barcode.create')}
          </DialogButton>
        </DialogFooter>
      )}
    >
      <div className="min-h-0 flex-1 overflow-y-auto bg-bb-bg/20 p-4">
        <DialogSection icon={<QrCode size={14} />} title={t('dialog.barcode.title')}>
          <div className="space-y-3">
	          <Select label={t('dialog.barcode.type')} value={barcodeType} options={barcodeOptions} onChange={(value) => setBarcodeType(value as BarcodeType)} selectClassName="w-36" />
          {barcodeType === 'qr_code' && (
            <div className="grid grid-cols-3 gap-1 rounded-lg bg-bb-bg p-1">
              {QR_PAYLOAD_MODES.map((mode) => (
                <button
                  key={mode}
                  type="button"
                  onClick={() => setQrPayloadMode(mode)}
                  className={`h-8 rounded-md border text-xs font-medium transition-colors ${qrPayloadMode === mode ? 'border-bb-accent/45 bg-bb-accent/10 text-bb-text' : 'border-transparent text-bb-text-muted hover:bg-bb-hover hover:text-bb-text'}`}
                >
                  {qrModeLabels[mode]}
                </button>
              ))}
            </div>
          )}
          {barcodeType !== 'qr_code' || qrPayloadMode === 'text' ? (
            <TextInput label={t('dialog.barcode.data')} value={data} onChange={setData} />
          ) : null}
          {barcodeType === 'qr_code' && qrPayloadMode === 'wifi' && (
            <>
              <TextInput label={t('dialog.barcode.ssid')} value={wifiSsid} onChange={setWifiSsid} />
              <TextInput label={t('dialog.barcode.password')} value={wifiPassword} onChange={setWifiPassword} />
              <Select label={t('dialog.barcode.security')} value={wifiSecurity} options={securityOptions} onChange={setWifiSecurity} selectClassName="w-36" />
            </>
          )}
          {barcodeType === 'qr_code' && qrPayloadMode === 'contact' && (
            <>
              <TextInput label={t('dialog.barcode.name')} value={contactName} onChange={setContactName} />
              <TextInput label={t('dialog.barcode.org')} value={contactOrg} onChange={setContactOrg} />
              <TextInput label={t('dialog.barcode.phone')} value={contactPhone} onChange={setContactPhone} />
              <TextInput label={t('dialog.barcode.email')} value={contactEmail} onChange={setContactEmail} />
            </>
          )}
          <NumberInput label={labelWithUnit(t('dialog.barcode.width'), lengthUnitLabel(displayUnit))} value={roundDisplayLength(mmToDisplay(width, displayUnit), displayUnit)} onChange={(v) => setWidth(displayToMm(v, displayUnit))} min={mmToDisplay(5, displayUnit)} max={mmToDisplay(500, displayUnit)} step={lengthStep(displayUnit, 1, 0.05)} />
          <NumberInput label={labelWithUnit(t('dialog.barcode.height'), lengthUnitLabel(displayUnit))} value={roundDisplayLength(mmToDisplay(height, displayUnit), displayUnit)} onChange={(v) => setHeight(displayToMm(v, displayUnit))} min={mmToDisplay(5, displayUnit)} max={mmToDisplay(500, displayUnit)} step={lengthStep(displayUnit, 1, 0.05)} />
          {isOneDimensional(barcodeType) && (
            <label className="flex min-h-8 cursor-pointer items-center justify-between gap-2 rounded-lg bg-bb-surface/35 px-2 text-xs text-bb-text-muted">
              {t('dialog.barcode.show_text')}
              <input className="accent-bb-accent" type="checkbox" checked={showText} onChange={(e) => setShowText(e.target.checked)} />
            </label>
          )}
          {barcodeType === 'qr_code' && (
            <Select label={t('dialog.barcode.qr_error')} value={qrErrorCorrection} options={qrEccOptions} onChange={(value) => setQrErrorCorrection(value as QrErrorCorrection)} selectClassName="w-36" />
          )}
          {barcodeType === 'data_matrix' && (
            <label className="flex min-h-8 cursor-pointer items-center justify-between gap-2 rounded-lg bg-bb-surface/35 px-2 text-xs text-bb-text-muted">
              {t('dialog.barcode.force_square')}
              <input className="accent-bb-accent" type="checkbox" checked={dataMatrixForceSquare} onChange={(e) => setDataMatrixForceSquare(e.target.checked)} />
            </label>
          )}
          </div>
        </DialogSection>
      </div>
    </MovableResizableDialogFrame>,
    document.body
  );
}
