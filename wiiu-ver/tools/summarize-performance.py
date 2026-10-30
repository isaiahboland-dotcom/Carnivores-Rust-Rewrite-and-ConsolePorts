#!/usr/bin/env python3
"""Summarize Wii U schema-1 through 11 captures, grouped by seed and output view modes."""
import argparse
import csv
import math
from collections import defaultdict
from pathlib import Path


def percentile(values, fraction):
    values = sorted(values)
    return values[max(0, math.ceil(len(values) * fraction) - 1)]


def summarize(path):
    groups = defaultdict(list)
    with path.open() as stream:
        for row in csv.DictReader(line for line in stream if not line.startswith('#')):
            groups[(row['seed'], row['tv_map'], row['drc_game'], row.get('death_playback', 'unknown'), row.get('tiled_atlases', '0'), row.get('static_cache_enabled', '0'), row.get('resident_enabled', '0'), row.get('vsync', '1'), row.get('tv_world', '?'))].append(
                {key: float(value) for key, value in row.items()})
    print(path)
    all_rows = sorted((r for rows in groups.values() for r in rows), key=lambda r: r['frame'])
    if all_rows:
        span = all_rows[-1]['start_ms'] - all_rows[0]['start_ms'] + all_rows[-1]['frame_ms']
        if span > 0:
            print(f'  whole capture: {len(all_rows)} frames over {span / 1000:.2f}s, '
                  f'{len(all_rows) * 1000 / span:.2f} wall FPS (includes gaps/menus)')
    for (seed, tv_map, drc_game, death, tiled, static_cache, resident, vsync, tv_world), rows in groups.items():
        count = len(rows)
        frames = [r['frame_ms'] for r in rows]
        total = sum(frames)
        overhead = sum(r['previous_profiler_ms'] for r in rows)
        # start_ms intervals also include SD I/O, death menus, pauses, and other gaps.
        span = rows[-1]['start_ms'] - rows[0]['start_ms'] + rows[-1]['frame_ms']
        print(f'  seed={seed} TV map toggle={tv_map} GamePad gameplay toggle={drc_game} death={death} tiled={tiled} static_cache={static_cache} resident={resident} vsync={vsync} tv_world={tv_world}: {count} frames')
        print(f'    timed-loop FPS={count * 1000 / total:.2f}, '
              f'average={total / count:.2f} ms, p95={percentile(frames, .95):.2f}, '
              f'p99={percentile(frames, .99):.2f}, max={max(frames):.2f}')
        print(f'    grouped wall span={span / 1000:.2f}s (may include gaps), '
              f'previous-profiler overhead={overhead:.2f} ms, '
              f'ticks/frame={sum(r["ticks"] for r in rows) / count:.2f}')
        buckets = [key for key in rows[0] if key.endswith('_ms') and key not in
                   ('start_ms', 'frame_ms', 'previous_profiler_ms', 'stream_lock_ms', 'stream_cache_ms')]
        for key in sorted(buckets, key=lambda k: sum(r[k] for r in rows), reverse=True):
            print(f'    {key}: {sum(r[key] for r in rows) / count:.3f} ms/frame')
        if 'resident_active' in rows[0]:
            print(f'    resident scenery active: {sum(r["resident_active"] for r in rows):.0f}/{count} frames, '
                  f'ranges/view={sum(r["scenery_ranges"] for r in rows) / count:.2f}, '
                  f'uploaded vertices/frame={sum(r["scenery_uploaded_v"] for r in rows) / count:.0f}')
        if 'static_reused' in rows[0]:
            print(f'    static geometry reused: {sum(r["static_reused"] for r in rows):.0f}/{count} frames')
        if 'stream_cache_ms' in rows[0]:
            print(f'    geometry subsets: lock={sum(r["stream_lock_ms"] for r in rows) / count:.3f} ms, '
                  f'cache={sum(r["stream_cache_ms"] for r in rows) / count:.3f} ms, '
                  f'cache range={sum(r["stream_cache_bytes"] for r in rows) / count / 1048576:.2f} MiB/frame')
        print(f'    emitted geometry streams: '
              f'{sum(r["geometry_stream_bytes"] for r in rows) / count / 1048576:.2f} MiB/frame')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('captures', nargs='+', type=Path)
    args = parser.parse_args()
    for capture in args.captures:
        summarize(capture)
