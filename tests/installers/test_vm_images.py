import importlib.util
from pathlib import Path
import unittest
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('vm_tests', ROOT / 'scripts/vm-tests.py')
vm = importlib.util.module_from_spec(spec)
spec.loader.exec_module(vm)


class VmImageTests(unittest.TestCase):
    """The images the VM tests boot are checked against their publishers."""

    def test_checksum_files_are_read_in_each_publishers_format(self):
        name = 'debian-13-genericcloud-amd64.qcow2'
        self.assertEqual(vm.parse_sums(f'abc123  {name}\nfff  other.qcow2\n', name), 'abc123')
        self.assertEqual(vm.parse_sums(f'abc123 *{name}\n', name), 'abc123')
        tagged = f'# {name}: 1 bytes\nSHA256 ({name}) = 0a1b\n'
        self.assertEqual(vm.parse_sums(tagged, name), '0a1b')
        self.assertIsNone(vm.parse_sums('abc123  something-else.qcow2\n', name))

    def test_fixed_releases_are_pinned_and_the_rest_come_from_their_projects(self):
        for key, image in vm.IMAGES.items():
            if 'digest' in image:
                # A pinned digest needs no file from the mirror channel.
                self.assertNotIn('sums', image, key)
                self.assertRegex(image['digest'], r'^[0-9a-f]{64}$', key)
                self.assertEqual(vm.expected_digest(image, 'x'), image['digest'])
            else:
                self.assertTrue(image['sums'].startswith('https://'), key)
                # Not the Fedora redirector, which hands both files to a mirror.
                self.assertNotIn('download.fedoraproject.org', image['sums'], key)
            self.assertTrue(image['url'].startswith('https://'), key)
        self.assertIn('digest', vm.IMAGES['fedora'])
        self.assertIn('digest', vm.IMAGES['fedora-arm64'])

    def test_mirrors_may_redirect_over_https_only(self):
        request = urllib.request.Request('https://download.example.org/image.qcow2')
        handler = vm.HttpsOnly()
        moved = handler.redirect_request(request, None, 302, 'Found', {}, 'https://mirror.example.net/image.qcow2')
        self.assertEqual(moved.full_url, 'https://mirror.example.net/image.qcow2')
        with self.assertRaises(urllib.error.HTTPError) as refused:
            handler.redirect_request(request, None, 302, 'Found', {}, 'http://mirror.example.net/image.qcow2')
        refused.exception.close()

    def test_a_checksum_is_not_taken_from_a_redirect(self):
        request = urllib.request.Request('https://cloud.example.org/SHA512SUMS')
        with self.assertRaises(urllib.error.HTTPError) as refused:
            vm.NoRedirect().redirect_request(request, None, 302, 'Found', {}, 'https://mirror.example.net/SHA512SUMS')
        refused.exception.close()


if __name__ == '__main__':
    unittest.main()
