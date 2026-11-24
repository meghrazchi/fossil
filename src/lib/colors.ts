import type { FileCategory } from './types';

export interface CategoryMeta {
  key: FileCategory;
  label: string;
  color: string;
  darkFill: string;
  border: string;
  badgeClass: string;
}

export const CATEGORY_LIST: CategoryMeta[] = [
  {
    key: 'video',
    label: 'Videos',
    color: '#F43F5E',
    darkFill: '#9F1239',
    border: '#FB7185',
    badgeClass: 'bg-rose-500/15 text-rose-300 border-rose-500/30',
  },
  {
    key: 'disk_image',
    label: 'Disk Images & VMs',
    color: '#A855F7',
    darkFill: '#6B21A8',
    border: '#C084FC',
    badgeClass: 'bg-purple-500/15 text-purple-300 border-purple-500/30',
  },
  {
    key: 'archive',
    label: 'Archives',
    color: '#F59E0B',
    darkFill: '#92400E',
    border: '#FBBF24',
    badgeClass: 'bg-amber-500/15 text-amber-300 border-amber-500/30',
  },
  {
    key: 'build_artifact',
    label: 'Code & Build',
    color: '#06B6D4',
    darkFill: '#155E75',
    border: '#22D3EE',
    badgeClass: 'bg-cyan-500/15 text-cyan-300 border-cyan-500/30',
  },
  {
    key: 'image',
    label: 'Images',
    color: '#10B981',
    darkFill: '#065F46',
    border: '#34D399',
    badgeClass: 'bg-emerald-500/15 text-emerald-300 border-emerald-500/30',
  },
  {
    key: 'audio',
    label: 'Audio',
    color: '#EC4899',
    darkFill: '#9D174D',
    border: '#F472B6',
    badgeClass: 'bg-pink-500/15 text-pink-300 border-pink-500/30',
  },
  {
    key: 'application',
    label: 'Apps & Binaries',
    color: '#6366F1',
    darkFill: '#3730A3',
    border: '#818CF8',
    badgeClass: 'bg-indigo-500/15 text-indigo-300 border-indigo-500/30',
  },
  {
    key: 'document',
    label: 'Documents',
    color: '#3B82F6',
    darkFill: '#1E40AF',
    border: '#60A5FA',
    badgeClass: 'bg-blue-500/15 text-blue-300 border-blue-500/30',
  },
  {
    key: 'system_cache',
    label: 'Logs & Caches',
    color: '#64748B',
    darkFill: '#334155',
    border: '#94A3B8',
    badgeClass: 'bg-slate-500/20 text-slate-300 border-slate-500/30',
  },
  {
    key: 'other',
    label: 'Other',
    color: '#71717A',
    darkFill: '#3F3F46',
    border: '#A1A1AA',
    badgeClass: 'bg-zinc-500/20 text-zinc-300 border-zinc-500/30',
  },
];

export const CATEGORY_BY_KEY: Record<FileCategory, CategoryMeta> = Object.fromEntries(
  CATEGORY_LIST.map((c) => [c.key, c]),
) as Record<FileCategory, CategoryMeta>;

export function getCategoryByIndex(index: number): CategoryMeta {
  return CATEGORY_LIST[index] ?? CATEGORY_LIST[CATEGORY_LIST.length - 1];
}
