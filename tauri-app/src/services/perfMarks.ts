function hasPerformanceMarks(): boolean {
  return (
    typeof performance !== 'undefined' &&
    typeof performance.mark === 'function' &&
    typeof performance.measure === 'function'
  );
}

let perfMarkSequence = 0;

function nextPerfMarkId(): number {
  perfMarkSequence += 1;
  return perfMarkSequence;
}

function tryMark(name: string): boolean {
  try {
    performance.mark(name);
    return true;
  } catch {
    return false;
  }
}

function tryMeasure(name: string, startMark: string, endMark: string): void {
  try {
    performance.measure(name, startMark, endMark);
  } catch {
    // Performance instrumentation must never fail the wrapped user action.
  }
}

function tryClearMarks(name: string): void {
  if (typeof performance.clearMarks !== 'function') return;
  try {
    performance.clearMarks(name);
  } catch {
    // Ignore browser-specific performance API failures.
  }
}

function tryClearMeasures(name: string): void {
  if (typeof performance.clearMeasures !== 'function') return;
  try {
    performance.clearMeasures(name);
  } catch {
    // Ignore browser-specific performance API failures.
  }
}

export async function measureAsyncPerf<T>(name: string, fn: () => Promise<T>): Promise<T> {
  if (!hasPerformanceMarks()) {
    return fn();
  }

  const markId = nextPerfMarkId();
  const startMark = `${name}:${markId}:start`;
  const endMark = `${name}:${markId}:end`;
  const hasStartMark = tryMark(startMark);
  try {
    return await fn();
  } finally {
    const hasEndMark = tryMark(endMark);
    if (hasStartMark && hasEndMark) {
      tryMeasure(name, startMark, endMark);
    }
    tryClearMarks(startMark);
    tryClearMarks(endMark);
    tryClearMeasures(name);
  }
}
