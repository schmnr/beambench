import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  libraryService,
  type LibraryProvider,
  type LibraryCollection,
  type LibraryFile,
} from '../../services/connectedLibraryService';
import { libraryStore } from '../../stores/connectedLibraryStore';
import { useProjectStore } from '../../stores/projectStore';
import { useUiStore } from '../../stores/uiStore';
const button =
  'rounded border border-bb-border px-3 py-1.5 text-xs text-bb-text hover:bg-bb-hover disabled:opacity-50';
function Preview({ file, provider }: { file: LibraryFile; provider: LibraryProvider }) {
  const [src, setSrc] = useState<string>();
  const node = useRef<HTMLDivElement>(null);
  useEffect(() => {
    let active = true;
    const observer = new IntersectionObserver(([entry]) => {
      if (!entry.isIntersecting) return;
      observer.disconnect();
      void libraryService(provider)
        .thumbnail(file)
        .then((value) => {
          if (active) setSrc(value);
        })
        .catch((error) => {
          if (active) libraryStore(provider).getState().reject(String(error));
        });
    });
    if (node.current) observer.observe(node.current);
    return () => {
      active = false;
      observer.disconnect();
    };
  }, [file, provider]);
  return (
    <div
      ref={node}
      className="flex h-28 items-center justify-center overflow-hidden rounded bg-bb-surface"
    >
      {src && <img src={src} alt="" className="max-h-full max-w-full object-contain" />}
    </div>
  );
}
export function CraftgineerPanel({
  connections = false,
  provider = 'craftgineer',
}: {
  connections?: boolean;
  provider?: LibraryProvider;
}) {
  const { t } = useTranslation();
  const prefix = provider === 'craftgineer' ? 'connected_library' : 'printcutcarve_library';
  const service = libraryService(provider);
  const useConnection = libraryStore(provider);
  const { status, busy, error, generation, restore, connect, cancel, disconnect } = useConnection();
  const account = status?.account;
  const needsCleanup = error?.includes('wallet_cleanup_required') ?? false;
  const [remember, setRemember] = useState(true);
  const [connecting, setConnecting] = useState(false);
  const [q, setQ] = useState('');
  const [packQuery, setPackQuery] = useState('');
  const [collection, setCollection] = useState<LibraryCollection | null>(null);
  const [collections, setCollections] = useState<LibraryCollection[]>([]);
  const [files, setFiles] = useState<LibraryFile[]>([]);
  const [unavailable, setUnavailable] = useState<{ id: string; name: string; reason: string }[]>(
    [],
  );
  const [cursor, setCursor] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [refresh, setRefresh] = useState(0);
  const sequence = useRef(0);
  const inserting = useRef(false);
  const fetchedAt = useRef(0);
  const hasProject = useProjectStore((s) => !!s.project);
  const packs = provider === 'printcutcarve' && !collection;
  const query = packs ? packQuery : q;
  useEffect(() => {
    setCollection(null);
    setQ('');
    setPackQuery('');
  }, [account?.account.id, provider]);
  useEffect(() => {
    if (connections && !needsCleanup && !useConnection.getState().status) void restore();
  }, [connections, needsCleanup, restore, useConnection]);
  useEffect(() => {
    const own = ++sequence.current;
    setFiles([]);
    setCollections([]);
    setUnavailable([]);
    setCursor(null);
    setMessage(null);
    if (connections || !account || account.access !== 'active') {
      setLoading(false);
      return;
    }
    setLoading(true);
    const timer = setTimeout(() => {
      const load = async () => {
        try {
          if (packs) {
            const result = await service.collections(query);
            if (own !== sequence.current || result.accountId !== account.account.id) return;
            setCollections(result.collections);
            setCursor(result.nextCursor);
          } else {
            const result = await service.files(query, null, collection?.id ?? null);
            if (own !== sequence.current || result.accountId !== account.account.id) return;
            setFiles(result.files);
            setUnavailable(result.unavailableFiles ?? []);
            setCursor(result.nextCursor);
          }
          fetchedAt.current = Date.now();
        } catch (error) {
          if (own === sequence.current) {
            setMessage(String(error));
            useConnection.getState().reject(String(error));
          }
        } finally {
          if (own === sequence.current) setLoading(false);
        }
      };
      void load();
    }, 300);
    return () => {
      clearTimeout(timer);
      if (sequence.current === own) sequence.current = own + 1;
    };
  }, [query, refresh, generation, account, connections, packs, collection, service, useConnection]);
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
      ...(provider === 'printcutcarve'
        ? ['file_too_large', 'unsupported_media_type', 'not_found']
        : []),
    ];
    const code =
      known.find((code) => value.includes(code)) ??
      (value.includes('stale_document') ? 'stale_document' : 'unavailable');
    return t(`${prefix}.errors.${code}`);
  };
  const openWebsite = () => {
    void service.openWebsite(collection?.slug ?? null).catch((error) => setMessage(String(error)));
  };
  const more = async () => {
    const own = sequence.current;
    setLoading(true);
    try {
      if (packs) {
        const result = await service.collections(query, cursor);
        if (own === sequence.current && result.accountId === account?.account.id) {
          setCollections((old) => [...old, ...result.collections]);
          setCursor(result.nextCursor);
        }
      } else {
        const result = await service.files(query, cursor, collection?.id ?? null);
        if (own === sequence.current && result.accountId === account?.account.id) {
          setFiles((old) => [...old, ...result.files]);
          setUnavailable((old) => [...old, ...(result.unavailableFiles ?? [])]);
          setCursor(result.nextCursor);
        }
      }
    } catch (error) {
      if (own === sequence.current) {
        setMessage(String(error));
        useConnection.getState().reject(String(error));
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
      await useProjectStore.getState().importConnectedLibraryFile(provider, file);
    } catch (error) {
      if (own === sequence.current) {
        setMessage(String(error));
        useConnection.getState().reject(String(error));
      }
    } finally {
      inserting.current = false;
      if (own === sequence.current) setLoading(false);
    }
  };
  return (
    <section className="space-y-3 p-3 text-xs text-bb-text" aria-label={t(`${prefix}.title`)}>
      {connections && (
        <>
          <h2 className="font-semibold">{t(`${prefix}.title`)}</h2>
          <p className="text-bb-text-muted">{t(`${prefix}.disclosure`)}</p>
          {!account && !needsCleanup && (
            <label className="flex items-center gap-2">
              <input
                type="checkbox"
                checked={remember}
                onChange={(e) => setRemember(e.target.checked)}
              />
              {t(`${prefix}.remember`)}
            </label>
          )}
          <div className="flex flex-wrap gap-2">
            {account || needsCleanup ? (
              <button className={button} disabled={busy} onClick={() => void disconnect()}>
                {t(`${prefix}.disconnect`)}
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
                {t(`${prefix}.connect`)}
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
                onClick={() => useUiStore.getState().setLibraryDrawerTab(provider)}
              >
                {t(`${prefix}.browse`)}
              </button>
            )}
            {!account && error && (
              <button className={button} disabled={busy} onClick={() => void restore()}>
                {t(`${prefix}.refresh`)}
              </button>
            )}
          </div>
          {account && status?.walletUnavailable && <p role="status">{t(`${prefix}.session_only`)}</p>}
        </>
      )}
      {account && (
        <p className="break-words text-bb-text-muted">
          {t(`${prefix}.account`, { account: account.account.displayName ?? account.account.id })}
        </p>
      )}
      {account?.access === 'subscription_required' && (
        <p>{t(`${prefix}.errors.subscription_required`)}</p>
      )}
      {(error || message) && <p role="alert">{showError(error ?? message!)}</p>}
      {busy && <p role="status">{t(connecting ? `${prefix}.waiting` : 'common.loading')}</p>}
      {!connections && account?.access === 'active' && (
        <>
          {collection && (
            <div className="space-y-2">
              <button className={button} onClick={() => setCollection(null)}>
                {t(`${prefix}.back_to_packs`)}
              </button>
              <h2 className="font-semibold">{collection.name}</h2>
            </div>
          )}
          <div className="flex gap-2">
            <input
              aria-label={t(`${prefix}.${packs ? 'pack_search' : 'search'}`)}
              placeholder={t(`${prefix}.${packs ? 'pack_search' : 'search'}`)}
              value={query}
              onChange={(e) => (packs ? setPackQuery(e.target.value) : setQ(e.target.value))}
              maxLength={120}
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
              {t(`${prefix}.refresh`)}
            </button>
          </div>
          {!hasProject && !packs && <p>{t(`${prefix}.open_project`)}</p>}
          {!loading &&
            !(packs ? collections.length : files.length + unavailable.length) &&
            !message && <p>{t(`${prefix}.${packs ? 'empty_packs' : 'empty'}`)}</p>}
          <div className="grid grid-cols-2 gap-3">
            {collections.map((item) => (
              <div key={item.id} className="min-w-0 space-y-2 rounded border border-bb-border p-2">
                {item.preview && <Preview provider={provider} file={item.preview} />}
                <p className="break-words">{item.name}</p>
                <p className="text-bb-text-muted">
                  {t(`${prefix}.file_count`, { count: item.fileCount })}
                </p>
                <button
                  className={button}
                  onClick={() => {
                    setCollection(item);
                    setQ('');
                  }}
                >
                  {t('menus.file.open')}
                </button>
              </div>
            ))}
            {files.map((file) => (
              <div
                key={`${generation}:${file.id}:${file.revision}`}
                className="min-w-0 space-y-2 rounded border border-bb-border p-2"
              >
                <Preview provider={provider} file={file} />
                <p className="break-words">{file.name}</p>
                <button
                  className={button}
                  disabled={loading || !hasProject}
                  onClick={() => void insert(file)}
                >
                  {t(`${prefix}.insert`)}
                </button>
              </div>
            ))}
            {unavailable.map((file) => (
              <div key={file.id} className="min-w-0 space-y-2 rounded border border-bb-border p-2">
                <p className="break-words">{file.name}</p>
                <p className="text-bb-text-muted">
                  {file.reason === 'file_too_large'
                    ? t(`${prefix}.errors.file_too_large`)
                    : t(`${prefix}.unavailable_file`)}
                </p>
                <button className={button} onClick={openWebsite}>
                  {t(`${prefix}.website`)}
                </button>
              </div>
            ))}
          </div>
          {loading && <p role="status">{t('common.loading')}</p>}
          {cursor && (
            <button className={button} disabled={loading} onClick={() => void more()}>
              {t(`${prefix}.more`)}
            </button>
          )}
          {provider === 'printcutcarve' && (
            <button className={button} onClick={openWebsite}>
              {t(`${prefix}.website`)}
            </button>
          )}
        </>
      )}
    </section>
  );
}
