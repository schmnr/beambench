#!/usr/bin/env python3
"""Check isolated test-driver origin, session and callback boundaries without network."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from urllib.parse import urlencode
import zipfile

spec = importlib.util.spec_from_file_location('library_driver', Path(__file__).with_name('test-connected-library-desktop.py'))
driver = importlib.util.module_from_spec(spec)
spec.loader.exec_module(driver)


class DriverBoundaries(unittest.TestCase):
    def test_empty_library_exception_is_only_for_an_unpaid_synthetic_second_account(self):
        account = {'id':'12345678-1234-1234-1234-123456789012',
                   'email':'pccbb-0123456789abcdef-b@example.invalid',
                   'label':'b','paidSubscription':False,'emptyLibrary':True,'pack':'','fixture':''}
        self.assertTrue(driver.valid_printcutcarve_fixture(account))
        for change in [{'email':'customer@example.com'}, {'paidSubscription':True},
                       {'label':'a'}, {'emptyLibrary':False}, {'fixture':'unexpected.svg'},
                       {'label':'a','paidSubscription':True,'pack':'Member pack','fixture':'member.svg'}]:
            self.assertFalse(driver.valid_printcutcarve_fixture({**account,**change}))

    def setUp(self):
        self.before = driver.PROVIDER, driver.HOST, driver.ORIGIN
        driver.PROVIDER, driver.HOST, driver.ORIGIN = 'printcutcarve', 'printcutcarve.com', 'https://printcutcarve.com'

    def tearDown(self):
        driver.PROVIDER, driver.HOST, driver.ORIGIN = self.before

    def test_disposable_session_is_secure_and_account_bound(self):
        account = driver.HostedAccount({'id': 'fixture-account', 'sessionToken': 'a' * 64})
        with patch.object(account, 'request', return_value=(200, {}, b'{"user":{"id":"fixture-account"}}')):
            account.login()
        cookie, = account.cookies
        self.assertEqual(cookie.domain, 'printcutcarve.com')
        self.assertEqual(cookie.name, '__Secure-next-auth.session-token')
        self.assertTrue(cookie.secure)
        self.assertTrue(cookie.has_nonstandard_attr('HttpOnly'))
        with patch.object(account, 'request', return_value=(200, {}, b'{"user":{"id":"another-account"}}')):
            with self.assertRaisesRegex(RuntimeError, '^disposable_session_rejected$'):
                account.login()

    def test_cross_provider_requests_and_callbacks_are_refused(self):
        account = driver.HostedAccount({})
        with self.assertRaisesRegex(RuntimeError, '^hosted_request_origin_rejected$'):
            account.request('https://craftgineer.com/api/auth/session')
        with self.assertRaisesRegex(RuntimeError, '^consent_origin_rejected$'):
            account.consent('https://craftgineer.com/integrations/desktop/authorize')
        query = urlencode({'client_id':'beambench-desktop', 'scope':'library:read', 'redirect_uri':'http://127.0.0.1:40000/oauth/craftgineer/callback'})
        with self.assertRaisesRegex(RuntimeError, '^callback_rejected$'):
            account.consent(driver.ORIGIN + '/integrations/desktop/authorize?' + query)

    def test_malformed_disposable_session_is_not_sent(self):
        account = driver.HostedAccount({'id': 'fixture-account', 'sessionToken': 'invalid'})
        with patch.object(account, 'request') as request:
            with self.assertRaisesRegex(RuntimeError, '^disposable_session_invalid$'):
                account.login()
            request.assert_not_called()

    def test_offline_proof_requires_a_readable_saved_project(self):
        with tempfile.TemporaryDirectory() as directory:
            project = Path(directory)/'test.lzrproj'
            self.assertIsNone(driver.project_geometry(project))
            project.write_bytes(b'incomplete project')
            self.assertIsNone(driver.project_geometry(project))
            with zipfile.ZipFile(project,'w') as archive:
                archive.writestr('project.json',json.dumps({'objects':[]}))
            self.assertIsNone(driver.project_geometry(project))

    def test_project_round_trip_compares_artwork_and_assets(self):
        with tempfile.TemporaryDirectory() as directory:
            project = Path(directory)/'test.lzrproj'
            geometry={'objects':[{'id':'shape','paths':[[0,0],[1,1]]}],'layers':[{'id':'line'}],'assets':[]}
            with zipfile.ZipFile(project,'w') as archive:
                archive.writestr('project.json',json.dumps({**geometry,'metadata':{'modified_at':'ignored'}}))
            self.assertEqual(driver.project_geometry(project),geometry)


if __name__ == '__main__':
    unittest.main()
