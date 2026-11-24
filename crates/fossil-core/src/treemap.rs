use crate::arena::{FileArena, NodeId, NodeKind, SizeMetric};
use crate::classify::FileCategory;

/// Binary record layout (32 bytes per rectangle, little-endian):
/// - bytes 0..4:   f32 x
/// - bytes 4..8:   f32 y
/// - bytes 8..12:  f32 w
/// - bytes 12..16: f32 h
/// - bytes 16..20: u32 `node_id`
/// - bytes 20..28: u64 size
/// - byte 28:      u8 category
/// - byte 29:      u8 depth
/// - byte 30:      u8 kind (0=file, 1=dir, 2=symlink, 3=other)
/// - byte 31:      u8 flags (bit 0 = `is_bloat`)
pub const RECT_STRIDE: usize = 32;
const MAX_RECTS: usize = 12_000;
const MIN_RECT_DIM: f32 = 2.0;
const HEADER_HEIGHT: f32 = 18.0;
const PAD: f32 = 1.5;

#[derive(Clone, Copy)]
struct Rect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

#[derive(Clone, Copy)]
struct Item {
    id: NodeId,
    size: u64,
}

/// Computes a squarified treemap layout in Rust and returns a compact binary `Vec<u8>` buffer
/// for zero-parse decoding on the Canvas frontend.
#[must_use]
pub fn compute_treemap_binary(
    arena: &FileArena,
    root_id: NodeId,
    width: f32,
    height: f32,
    metric: SizeMetric,
    category_filter: Option<FileCategory>,
    max_depth: u8,
) -> Vec<u8> {
    if width < 4.0 || height < 4.0 || arena.nodes.get(root_id as usize).is_none() {
        return Vec::new();
    }

    let mut out = Vec::with_capacity(4096 * RECT_STRIDE);
    let bounds = Rect {
        x: 0.0,
        y: 0.0,
        w: width,
        h: height,
    };

    layout_children(
        arena,
        root_id,
        bounds,
        0,
        max_depth.clamp(1, 5),
        metric,
        category_filter,
        &mut out,
    );

    out
}

#[allow(clippy::too_many_arguments)]
fn layout_children(
    arena: &FileArena,
    parent_id: NodeId,
    bounds: Rect,
    depth: u8,
    max_depth: u8,
    metric: SizeMetric,
    category_filter: Option<FileCategory>,
    out: &mut Vec<u8>,
) {
    if out.len() / RECT_STRIDE >= MAX_RECTS || bounds.w < MIN_RECT_DIM || bounds.h < MIN_RECT_DIM {
        return;
    }

    let parent = &arena.nodes[parent_id as usize];
    if parent.children.is_empty() {
        return;
    }

    let mut items: Vec<Item> = Vec::with_capacity(parent.children.len().min(512));
    let mut total_size: u64 = 0;

    for &cid in &parent.children {
        let child = &arena.nodes[cid as usize];
        if child.is_deleted() {
            continue;
        }
        if let Some(cat) = category_filter
            && !child.is_dir()
            && child.category != cat
        {
            continue;
        }
        let sz = child.size(metric);
        if sz == 0 {
            continue;
        }
        total_size = total_size.saturating_add(sz);
        items.push(Item { id: cid, size: sz });
    }

    if total_size == 0 || items.is_empty() {
        return;
    }

    // Ensure descending order for squarification (already sorted for Disk, re-sort for Logical)
    if metric == SizeMetric::Logical && items.len() > 1 {
        items.sort_unstable_by_key(|b| std::cmp::Reverse(b.size));
    }

    squarify(
        arena,
        &items,
        total_size,
        bounds,
        depth,
        max_depth,
        metric,
        category_filter,
        out,
    );
}

#[allow(clippy::too_many_arguments)]
fn squarify(
    arena: &FileArena,
    items: &[Item],
    mut remaining_size: u64,
    mut bounds: Rect,
    depth: u8,
    max_depth: u8,
    metric: SizeMetric,
    category_filter: Option<FileCategory>,
    out: &mut Vec<u8>,
) {
    let mut idx = 0;
    while idx < items.len() && remaining_size > 0 && out.len() / RECT_STRIDE < MAX_RECTS {
        if bounds.w < MIN_RECT_DIM || bounds.h < MIN_RECT_DIM {
            break;
        }

        let horizontal = bounds.w >= bounds.h;
        let short_side = if horizontal { bounds.h } else { bounds.w };
        let total_area = bounds.w * bounds.h;
        let scale = total_area / remaining_size as f32;

        let mut row_end = idx + 1;
        let mut row_sum = items[idx].size;
        let mut best_worst = worst_aspect(
            items[idx..row_end].iter().map(|i| i.size as f32 * scale),
            row_sum as f32 * scale,
            short_side,
        );

        while row_end < items.len() {
            let next_sum = row_sum + items[row_end].size;
            let next_worst = worst_aspect(
                items[idx..=row_end].iter().map(|i| i.size as f32 * scale),
                next_sum as f32 * scale,
                short_side,
            );
            if next_worst <= best_worst {
                row_sum = next_sum;
                best_worst = next_worst;
                row_end += 1;
            } else {
                break;
            }
        }

        let row_area = row_sum as f32 * scale;
        let row_thickness = (row_area / short_side).max(0.0);

        let mut offset = 0.0f32;
        for item in &items[idx..row_end] {
            if out.len() / RECT_STRIDE >= MAX_RECTS {
                break;
            }
            let frac = item.size as f32 / row_sum as f32;
            let len = frac * short_side;
            let rect = if horizontal {
                Rect {
                    x: bounds.x,
                    y: bounds.y + offset,
                    w: row_thickness,
                    h: len,
                }
            } else {
                Rect {
                    x: bounds.x + offset,
                    y: bounds.y,
                    w: len,
                    h: row_thickness,
                }
            };
            offset += len;

            if rect.w < MIN_RECT_DIM || rect.h < MIN_RECT_DIM {
                continue;
            }

            let node = &arena.nodes[item.id as usize];
            push_rect(
                out,
                rect,
                item.id,
                item.size,
                node.category as u8,
                depth,
                node.kind,
                node.flags.bits() as u8,
            );

            // Recurse into large enough directories to show nested structure
            if node.kind == NodeKind::Directory
                && depth + 1 < max_depth
                && rect.w > 28.0
                && rect.h > HEADER_HEIGHT + 14.0
            {
                let inner = Rect {
                    x: rect.x + PAD,
                    y: rect.y + HEADER_HEIGHT,
                    w: rect.w - PAD * 2.0,
                    h: rect.h - HEADER_HEIGHT - PAD,
                };
                layout_children(
                    arena,
                    item.id,
                    inner,
                    depth + 1,
                    max_depth,
                    metric,
                    category_filter,
                    out,
                );
            }
        }

        // Shrink remaining bounds
        if horizontal {
            bounds.x += row_thickness;
            bounds.w = (bounds.w - row_thickness).max(0.0);
        } else {
            bounds.y += row_thickness;
            bounds.h = (bounds.h - row_thickness).max(0.0);
        }

        remaining_size = remaining_size.saturating_sub(row_sum);
        idx = row_end;
    }
}

#[inline]
fn worst_aspect(areas: impl Iterator<Item = f32>, sum_area: f32, side: f32) -> f32 {
    if sum_area <= 0.0 || side <= 0.0 {
        return f32::INFINITY;
    }
    let s2 = side * side;
    let sum2 = sum_area * sum_area;
    let mut worst = 0.0f32;
    for a in areas {
        if a <= 0.0 {
            continue;
        }
        let r1 = (s2 * a) / sum2;
        let r2 = sum2 / (s2 * a);
        let ratio = r1.max(r2);
        if ratio > worst {
            worst = ratio;
        }
    }
    worst
}

#[inline]
#[allow(clippy::too_many_arguments)]
fn push_rect(
    out: &mut Vec<u8>,
    r: Rect,
    node_id: NodeId,
    size: u64,
    category: u8,
    depth: u8,
    kind: NodeKind,
    flags: u8,
) {
    out.extend_from_slice(&r.x.to_le_bytes());
    out.extend_from_slice(&r.y.to_le_bytes());
    out.extend_from_slice(&r.w.to_le_bytes());
    out.extend_from_slice(&r.h.to_le_bytes());
    out.extend_from_slice(&node_id.to_le_bytes());
    out.extend_from_slice(&size.to_le_bytes());
    out.push(category);
    out.push(depth);
    out.push(kind as u8);
    out.push(flags);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::arena::NodeFlags;
    use std::path::PathBuf;

    #[test]
    fn computes_binary_treemap_rects() {
        let mut arena = FileArena::new(PathBuf::from("/tmp/root"));
        arena.push_node(0, "a.mp4", NodeKind::File, 600, 600, 0, NodeFlags::empty());
        arena.push_node(0, "b.zip", NodeKind::File, 400, 400, 0, NodeFlags::empty());
        arena.finalize();

        let bytes = compute_treemap_binary(&arena, 0, 800.0, 600.0, SizeMetric::Disk, None, 2);
        assert_eq!(bytes.len(), 2 * RECT_STRIDE);
    }
}
