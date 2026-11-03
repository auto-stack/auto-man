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


class ThemeStagingTests(unittest.TestCase):
    """PLAN-036：官方主题包 staging 核验与 Core/full profile 反控（真实文件系统）。"""

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        self.seq = 0

    def write_theme_pkg(self, base, *, version='0.2.0', tamper=None, extra=None, drop=None):
        pkg = base / 'sdk/packages/themes'
        pkg.mkdir(parents=True)
        bodies = {'pac.at': 'name: "themes"\nversion: "0.2.0"\n',
                  'themes.at': 'pub theme Auto { }\npub theme Solarized { extend "Auto" { } }\n'}
        for name, body in bodies.items():
            (pkg / name).write_text(body)
        entries = [{'path': name, 'sha256': installer.sha256(pkg / name)} for name in sorted(bodies)]
        if drop:  # 清单仍登记、文件已消失 = 缺件态
            (pkg / drop).unlink()
        for name in (tamper or ()):  # 内容改、清单 hash 不改 = 篡改态
            (pkg / name).write_text('tampered-content\n')
        if extra:
            (pkg / extra).write_text('unregistered\n')
        manifest = {'owner': 'auto-ui', 'package': 'themes', 'version': version, 'schema': 1,
                    'files': entries}
        (pkg / 'theme-package.json').write_text(json.dumps(manifest))
        return pkg

    def make_install(self, profile, *, theme=True, **theme_kwargs):
        self.seq += 1
        root = self.root / f'inst-{profile}-theme-{int(theme)}-{self.seq}'
        (root / 'sdk/crates/auto-core').mkdir(parents=True)
        (root / 'sdk/crates/auto/src').mkdir(parents=True)
        (root / 'auto.exe').write_bytes(b'')
        (root / 'sdk/crates/auto-core/Cargo.toml').write_text('[package]\n')
        (root / 'sdk/crates/auto/src/main.rs').write_text('fn main() {}\n')
        (root / 'sdk/crates/auto/build.rs').write_text('fn main() {}\n')
        if profile == 'full':
            (root / 'sdk/crates/auto-ui-tools').mkdir(parents=True)
            (root / 'sdk/crates/auto-ui-tools/Cargo.toml').write_text('[package]\n')
            (root / 'auto-ui-tool.exe').write_bytes(b'')
        if theme:
            self.write_theme_pkg(root, **theme_kwargs)
        sources = (['auto-man', 'auto-lang'] if profile == 'core'
                   else ['auto-man', 'auto-lang', 'auto-ui', 'auto-down'])
        doc = {'schema': 'man-install/1', 'profile': profile,
               'sources': {name: 'head' for name in sources},
               'files': installer.manifest_files(root)}
        (root / installer.MANIFEST).write_text(json.dumps(doc))
        return root

    def test_official_theme_manifest_accepts_self_consistent_package(self):
        pkg = self.write_theme_pkg(self.root)
        result = installer.verify_official_theme_manifest(pkg)
        self.assertEqual(result['package'], 'themes')
        self.assertEqual(result['version'], '0.2.0')
        self.assertEqual(result['files'], 3)

    def test_theme_manifest_named_rejections(self):
        pkg = self.write_theme_pkg(self.root / 'a', version='0.3.0')
        with self.assertRaisesRegex(RuntimeError, 'version'):
            installer.verify_official_theme_manifest(pkg)
        pkg = self.write_theme_pkg(self.root / 'b', tamper=('themes.at',))
        with self.assertRaisesRegex(RuntimeError, 'hash mismatch'):
            installer.verify_official_theme_manifest(pkg)
        pkg = self.write_theme_pkg(self.root / 'c', drop='themes.at')
        with self.assertRaisesRegex(RuntimeError, 'missing'):
            installer.verify_official_theme_manifest(pkg)
        # 清单外文件（如源包 README）在 staging 核验中合法——快照纯度归 UI
        # 发布原语；安装后多件由 verify() 全树对账拒绝。
        pkg = self.write_theme_pkg(self.root / 'd', extra='README.md')
        installer.verify_official_theme_manifest(pkg)
        pkg = self.write_theme_pkg(self.root / 'e')
        (pkg / 'theme-package.json').unlink()
        with self.assertRaisesRegex(RuntimeError, 'unreadable'):
            installer.verify_official_theme_manifest(pkg)

    def test_full_verify_requires_theme_and_core_rejects_themes(self):
        full = self.make_install('full', theme=True)
        self.assertEqual(installer.verify(full, 'full')['profile'], 'full')
        full_missing = self.make_install('full', theme=False)
        with self.assertRaisesRegex(RuntimeError, 'full profile required input missing'):
            installer.verify(full_missing, 'full')
        core = self.make_install('core', theme=False)
        self.assertEqual(installer.verify(core, 'core')['profile'], 'core')
        core_with = self.make_install('core', theme=True)
        with self.assertRaisesRegex(RuntimeError, 'UI source/tool in Core install'):
            installer.verify(core_with, 'core')
        # 安装后篡改主题文件 → 全树 hash 对账具名拒绝（verify 免费覆盖）。
        self.write_theme_pkg(self.root / 'post', extra=None)
        planted = self.make_install('full', theme=True)
        (planted / 'sdk/packages/themes/themes.at').write_text('mutated\n')
        with self.assertRaisesRegex(RuntimeError, 'install content mismatch'):
            installer.verify(planted, 'full')


if __name__ == '__main__':
    unittest.main()
