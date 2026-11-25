"""Android gfxinfo app frame-processing statistics. Not scanout FPS or game benchmarks."""
import csv
import statistics


def summarize(text):
    header = None
    samples = []
    for line in text.splitlines():
        columns = next(csv.reader([line.strip()]))
        if 'Flags' in columns and 'IntendedVsync' in columns and 'FrameCompleted' in columns:
            header = {name: index for index, name in enumerate(columns)}
            continue
        if not header:
            continue
        try:
            flags = int(columns[header['Flags']])
            start = int(columns[header['IntendedVsync']])
            end = int(columns[header['FrameCompleted']])
        except (ValueError, IndexError):
            continue
        if flags != 0 or start <= 0 or end <= start or end >= 9223372036854775807:
            continue
        samples.append((end - start) / 1_000_000)
    samples.sort()
    if not samples:
        return {'status': 'insufficient_samples', 'frames': 0, 'scope': 'Settings app frame-processing times; not game FPS or physical display rate'}
    def percentile(value):
        return samples[min(len(samples) - 1, int((len(samples) - 1) * value + 0.5))]
    return {
        'status': 'measured' if len(samples) >= 5 else 'insufficient_samples',
        'scope': 'Settings app frame-processing times; not game FPS or physical display rate',
        'frames': len(samples), 'median_ms': round(statistics.median(samples), 3),
        'p95_ms': round(percentile(.95), 3), 'p99_ms': round(percentile(.99), 3),
        'max_ms': round(samples[-1], 3),
        'over_16_67ms': sum(value > 16.67 for value in samples),
        'reference_budget_note': '16.67 ms is a 60 Hz reference, not a detected host monitor rate',
    }
