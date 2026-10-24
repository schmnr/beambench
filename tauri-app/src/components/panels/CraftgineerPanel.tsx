import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  connectedLibraryService as service,
  type LibraryFile,
} from '../../services/connectedLibraryService';
import { useConnectedLibraryStore } from '../../stores/connectedLibraryStore';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';
const button =
  'rounded border border-bb-border px-3 py-1.5 text-xs text-bb-text hover:bg-bb-hover disabled:opacity-50';
function Preview({ file }: { file: LibraryFile }) {
  const [src, setSrc] = useState<string>();
  const node = useRef<HTMLDivElement>(null);
  useEffect(() => {
    let active = true;
    const observer = new IntersectionObserver(([entry]) => {
      if (!entry.isIntersecting) return;
      observer.disconnect();
      void service
        .thumbnail(file)
        .then((value) => {
          if (active) setSrc(value);
        })
        .catch((error) => {
          if (active) useConnectedLibraryStore.getState().reject(String(error));
        });
    });
    if (node.current) observer.observe(node.current);
    return () => {
      active = false;
      observer.disconnect();
    };
  }, [file]);
  return (
    <div
      ref={node}
      className="flex h-28 items-center justify-center overflow-hidden rounded bg-bb-surface"
    >
      {src && <img src={src} alt="" className="max-h-full max-w-full object-contain" />}
    </div>
  );
}
export function CraftgineerPanel({ connections = false }: { connections?: boolean }) {
  const { t } = useTranslation();
  const { status, busy, error, generation, restore, connect, cancel, disconnect } =
    useConnectedLibraryStore();
  const account = status?.account;
  const [remember, setRemember] = useState(true);
  const [connecting, setConnecting] = useState(false);
  const [q, setQ] = useState('');
  const [files, setFiles] = useState<LibraryFile[]>([]);
  const [cursor, setCursor] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [refresh, setRefresh] = useState(0);
  const sequence = useRef(0);
  const inserting = useRef(false);
  const fetchedAt = useRef(0);
  const hasProject = useProjectStore((s) => !!s.project);
  useEffect(() => {
    if (connections && !useConnectedLibraryStore.getState().status) void restore();
  }, [connections, restore]);
  useEffect(() => {
    const own = ++sequence.current;
    setFiles([]);
    setCursor(null);
    setMessage(null);
    if (connections || !account || account.access !== 'active') {
      setLoading(false);
      return;
    }
    setLoading(true);
    const timer = setTimeout(() => {
      void service
        .files(q)
        .then((result) => {
          if (own !== sequence.current || result.accountId !== account.account.id) return;
          fetchedAt.current = Date.now();
          setFiles(result.files);
          setCursor(result.nextCursor);
        })
        .catch((error) => {
          if (own === sequence.current) {
            setMessage(String(error));
            useConnectedLibraryStore.getState().reject(String(error));
          }
        })
        .finally(() => {
          if (own === sequence.current) setLoading(false);
        });
    }, 300);
    return () => {
      clearTimeout(timer);
      if (sequence.current === own) sequence.current = own + 1;
    };
  }, [q, refresh, generation, account, connections]);
  useEffect(() => {
    if (connections || !account) return;
    const onFocus = () => {
      if (Date.now() - fetchedAt.current < 30_000) return;
      fetchedAt.current = Date.now();
      void restore();
      setRefresh((value) => value + 1);
    };
    window.addEventListener('focus', onFocus);
    return () => window.removeEventListener('focus', onFocus);
  }, [account, connections, restore]);
  const showError = (value: string) => {
    const known = [
      'subscription_required',
      'invalid_token',
      'revision_conflict',
      'access_denied',
      'connection_timeout',
      'wallet_cleanup_required',
      'revocation_pending',
      'rate_limited',
    ];
    const code =
      known.find((code) => value.includes(code)) ??
      (value.includes('stale_document') ? 'stale_document' : 'unavailable');
    return t(`connected_library.errors.${code}`);
  };
  const more = async () => {
    const own = sequence.current;
    setLoading(true);
    try {
      const result = await service.files(q, cursor);
      if (own === sequence.current && result.accountId === account?.account.id) {
        setFiles((old) => [...old, ...result.files]);
        setCursor(result.nextCursor);
      }
    } catch (error) {
      if (own === sequence.current) {
        setMessage(String(error));
        useConnectedLibraryStore.getState().reject(String(error));
      }
    } finally {
      if (own === sequence.current) setLoading(false);
    }
  };
  const insert = async (file: LibraryFile) => {
    if (inserting.current) return;
    inserting.current = true;
    const own = sequence.current;
    setLoading(true);
    setMessage(null);
    try {
      await useProjectStore.getState().importCraftgineerFile(file);
    } catch (error) {
      if (own === sequence.current) {
        setMessage(String(error));
        useConnectedLibraryStore.getState().reject(String(error));
      }
    } finally {
      inserting.current = false;
      if (own === sequence.current) setLoading(false);
    }
  };
  return (
    <section
      className="space-y-3 p-3 text-xs text-bb-text"
      aria-label={t('connected_library.title')}
    >
      {connections && (
        <>
          <h2 className="font-semibold">{t('connected_library.title')}</h2>
          <p className="text-bb-text-muted">{t('connected_library.disclosure')}</p>
          {!account && (
            <label className="flex items-center gap-2">
              <input
                type="checkbox"
                checked={remember}
                onChange={(e) => setRemember(e.target.checked)}
              />
              {t('connected_library.remember')}
            </label>
          )}
          <div className="flex flex-wrap gap-2">
            {account ? (
              <button className={button} disabled={busy} onClick={() => void disconnect()}>
                {t('connected_library.disconnect')}
              </button>
            ) : (
              <button
                className={button}
                disabled={busy}
                onClick={() => {
                  setConnecting(true);
                  void connect(remember).finally(() => setConnecting(false));
                }}
              >
                {t('connected_library.connect')}
              </button>
            )}
            {busy && connecting && (
              <button className={button} onClick={() => void cancel()}>
                {t('common.cancel')}
              </button>
            )}
            {account && (
              <button
                className={button}
                onClick={() => useUiStore.getState().setLibraryDrawerTab('craftgineer')}
              >
                {t('connected_library.browse')}
              </button>
            )}
          </div>
          {status?.walletUnavailable && <p role="status">{t('connected_library.session_only')}</p>}
        </>
      )}
      {account && (
        <p className="break-words text-bb-text-muted">
          {t('connected_library.account', {
            account: account.account.displayName ?? account.account.id,
          })}
        </p>
      )}
      {account?.access === 'subscription_required' && (
        <p>{t('connected_library.errors.subscription_required')}</p>
      )}
      {(error || message) && <p role="alert">{showError(error ?? message!)}</p>}
      {busy && (
        <p role="status">{t(connecting ? 'connected_library.waiting' : 'common.loading')}</p>
      )}
      {!connections && account?.access === 'active' && (
        <>
          <div className="flex gap-2">
            <input
              aria-label={t('connected_library.search')}
              placeholder={t('connected_library.search')}
              value={q}
              maxLength={120}
              onChange={(e) => setQ(e.target.value)}
              className="min-w-0 flex-1 rounded border border-bb-border bg-bb-surface px-2 py-1.5"
            />
            <button
              className={button}
              disabled={loading}
              onClick={() => {
                void restore();
                setRefresh((old) => old + 1);
              }}
            >
              {t('connected_library.refresh')}
            </button>
          </div>
          {!hasProject && <p>{t('connected_library.open_project')}</p>}
          {!loading && files.length === 0 && !message && <p>{t('connected_library.empty')}</p>}
          <div className="grid grid-cols-2 gap-3">
            {files.map((file) => (
              <div
                key={`${generation}:${file.id}:${file.revision}`}
                className="min-w-0 space-y-2 rounded border border-bb-border p-2"
              >
                <Preview file={file} />
                <p className="break-words">{file.name}</p>
                <button
                  className={button}
                  disabled={loading || !hasProject}
                  onClick={() => void insert(file)}
                >
                  {t('connected_library.insert')}
                </button>
              </div>
            ))}
          </div>
          {loading && <p role="status">{t('common.loading')}</p>}
          {cursor && (
            <button className={button} disabled={loading} onClick={() => void more()}>
              {t('connected_library.more')}
            </button>
          )}
        </>
      )}
    </section>
  );
}
