"""Real filesystem counterexamples for the shared Man/Web install transaction."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('man_install', Path(__file__).with_name('man-install.py'))
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


class InstallTransactionTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name) / 'installed'

    def prepare(self, version):
        def write(stage):
            (stage / 'tool.txt').write_text(version)
            doc = {'files': installer.manifest_files(stage)}
            (stage / installer.MANIFEST).write_text(json.dumps(doc))
        return write

    def verify(self, root):
        doc = json.loads((root / installer.MANIFEST).read_text())
        if installer.manifest_files(root) != doc['files']:
            raise RuntimeError('payload hash mismatch')
        return {'version': (root / 'tool.txt').read_text()}

    def install(self, version, fail=None, verify=None):
        return installer.install_payload(self.root, self.prepare(version), verify or self.verify,
                                         fail, verify_existing=self.verify)

    def test_upgrade_keeps_verified_backup_and_installs_new_payload(self):
        self.install('old')
        result = self.install('new')
        self.assertEqual(self.verify(self.root)['version'], 'new')
        backup = Path(result['backup_root'])
        (backup / '.man-backup.marker').unlink()
        self.assertEqual(self.verify(backup)['version'], 'old')

    def test_each_failure_before_swap_preserves_old_usable_payload(self):
        self.install('old')
        before = installer.manifest_files(self.root)
        for failure in ('prepare', 'verify', 'swap'):
            with self.subTest(failure=failure), self.assertRaises(RuntimeError):
                self.install('new', failure)
            self.assertEqual(installer.manifest_files(self.root), before)
            self.assertEqual(self.verify(self.root)['version'], 'old')
        self.assertFalse(list(self.root.parent.glob('.installed.man-staging-*')))

    def test_post_swap_verification_failure_restores_old_and_preserves_failed_new(self):
        self.install('old')
        def reject_new_at_destination(path):
            result = self.verify(path)
            if path == self.root and result['version'] == 'new':
                raise RuntimeError('post-swap consumer rejection')
            return result
        with self.assertRaises(RuntimeError): self.install('new', verify=reject_new_at_destination)
        self.assertEqual(self.verify(self.root)['version'], 'old')
        failed = list(self.root.parent.glob('.installed.man-failed-*'))
        self.assertEqual(len(failed), 1)
        self.assertEqual(self.verify(failed[0])['version'], 'new')

    def test_unmanaged_and_damaged_roots_reject_before_preparation(self):
        self.root.mkdir()
        sentinel = self.root / 'user.txt'
        sentinel.write_text('user data')
        with self.assertRaises(RuntimeError): self.install('new')
        self.assertEqual(sentinel.read_text(), 'user data')
        sentinel.unlink()
        self.install('old')
        (self.root / 'tool.txt').write_text('tampered')
        with self.assertRaises(RuntimeError): self.install('new')
        self.assertEqual((self.root / 'tool.txt').read_text(), 'tampered')

    def test_live_unknown_and_unmarked_staging_is_preserved(self):
        for kind, alive in (('live', True), ('unknown', None), ('dead', False)):
            stage = self.root.with_name('.installed.man-staging-' + kind)
            stage.mkdir()
            (stage / '.man-staging.marker').write_text(json.dumps({'schema': 'man-staging/1', 'pid': 123}))
            with patch.object(installer, 'process_alive', return_value=alive):
                installer.recover_stale_staging(self.root)
            self.assertEqual(stage.exists(), alive is not False)
        unknown = self.root.with_name('.installed.man-staging-unmarked')
        unknown.mkdir()
        installer.recover_stale_staging(self.root)
        self.assertTrue(unknown.exists())

    def test_nested_receipts_are_hashed_and_rebuildable_caches_are_excluded(self):
        self.install('old')
        cache = self.root / 'sdk/target/debug/cache'
        cache.parent.mkdir(parents=True)
        cache.write_text('rebuildable')
        self.verify(self.root)
        nested = self.root / 'sdk/install-manifest.json'
        nested.write_text('unregistered nested input')
        with self.assertRaises(RuntimeError): self.verify(self.root)


if __name__ == '__main__':
    unittest.main()
