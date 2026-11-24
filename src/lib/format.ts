const UNITS = ['B', 'KB', 'MB', 'GB', 'TB', 'PB'];

export function formatBytes(bytes: number, decimals = 2): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B';
  const k = 1000;
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(k)), UNITS.length - 1);
  const val = bytes / Math.pow(k, i);
  const digits = i === 0 ? 0 : val >= 100 ? 1 : decimals;
  return `${val.toFixed(digits)} ${UNITS[i]}`;
}

export function formatCount(n: number): string {
  return new Intl.NumberFormat('en-US').format(Math.max(0, Math.round(n)));
}

export function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms} ms`;
  const sec = ms / 1000;
  if (sec < 60) return `${sec.toFixed(2)} s`;
  const mins = Math.floor(sec / 60);
  const rem = (sec % 60).toFixed(0).padStart(2, '0');
  return `${mins}m ${rem}s`;
}

export function formatDate(unixSeconds: number): string {
  if (!unixSeconds || unixSeconds <= 0) return '-';
  const d = new Date(unixSeconds * 1000);
  return d.toISOString().slice(0, 10);
}

export function formatPercent(pct: number): string {
  if (!Number.isFinite(pct) || pct <= 0) return '0%';
  if (pct < 0.1) return '<0.1%';
  if (pct >= 10) return `${pct.toFixed(1)}%`;
  return `${pct.toFixed(2)}%`;
}
