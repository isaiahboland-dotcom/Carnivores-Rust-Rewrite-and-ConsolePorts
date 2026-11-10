"""Comparison-summary tests use synthetic metrics, without game media."""
import json
from pathlib import Path
import tempfile
import unittest

from texture_review_summary import distribution, summarize, inject, refresh, START


class SummaryTests(unittest.TestCase):
    def test_capture_weighted_average_percentiles_and_threshold_boundary(self):
        stats = distribution([.01, .02, .03, .10], .02)
        self.assertAlmostEqual(stats['mean'], .04)
        self.assertAlmostEqual(stats['median'], .025)
        self.assertEqual(stats['p95'], .10)
        self.assertEqual(stats['maximum'], .10)
        self.assertEqual(stats['exceeding'], 2)
        self.assertIsNone(distribution([], .02)['mean'])

    def test_invalid_missing_and_nonfinite_metrics_do_not_lower_the_average(self):
        review = dict(engine='original-software', allowed_changed_fraction=.005, captures=[
            dict(source='a', parity_changed_fraction=.02),
            dict(source='b', parity_changed_fraction=.04),
            dict(source='a', parity_changed_fraction=0, capture_error='empty'),
            dict(source='a', parity_changed_fraction=float('nan')),
            dict(source='a')])
        stats = summarize(review)
        self.assertAlmostEqual(stats['mean'], .03)
        self.assertEqual(stats['count'], 2)
        self.assertEqual(stats['excluded'], 3)
        self.assertEqual(stats['by_source']['a']['mean'], .02)

    def test_refresh_is_idempotent_preserves_threshold_and_media_and_refuses_archived_runs(self):
        with tempfile.TemporaryDirectory() as tmp:
            run = Path(tmp)
            review = dict(kind='private-texture-review', engine='rust', human_approval='pending',
                          allowed_changed_fraction=.005, channel_tolerance=12,
                          captures=[dict(source='a', changed_fraction=.025)])
            (run/'review.json').write_text(json.dumps(review))
            (run/'report.html').write_text('<h1>Review</h1><img src="existing.png">')
            (run/'existing.png').write_bytes(b'unchanged synthetic media')
            refresh(run); refresh(run)
            result = (run/'report.html').read_text()
            self.assertEqual(result.count(START), 1)
            self.assertIn('2.50%', result)
            self.assertIn('0.50%', result)
            self.assertIn('<img src="existing.png">', result)
            self.assertEqual((run/'existing.png').read_bytes(), b'unchanged synthetic media')
            self.assertEqual(json.loads((run/'review.json').read_text())['allowed_changed_fraction'], .005)
            (run/'archive.json').write_text('{}')
            with self.assertRaises(ValueError): refresh(run)

    def test_combined_index_weights_captures_not_game_means(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for name, values in [('a', [.02]), ('b', [.04, .04, .04])]:
                child = root/name; child.mkdir()
                (child/'review.json').write_text(json.dumps(dict(
                    allowed_changed_fraction=.005, channel_tolerance=12,
                    captures=[dict(source=name, parity_changed_fraction=v) for v in values])))
            index = root/'index'; index.mkdir()
            (index/'index.json').write_text(json.dumps(dict(kind='private-original-texture-index',
                human_approval='pending', runs=[dict(run='a'), dict(run='b')])))
            (index/'report.html').write_text('<h1>Combined</h1>')
            self.assertAlmostEqual(refresh(index)['mean'], .035)
