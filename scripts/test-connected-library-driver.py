#!/usr/bin/env python3
"""Check isolated test-driver origin, session and callback boundaries without network."""
import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch
from urllib.parse import urlencode

spec = importlib.util.spec_from_file_location('library_driver', Path(__file__).with_name('test-connected-library-desktop.py'))
driver = importlib.util.module_from_spec(spec)
spec.loader.exec_module(driver)


class DriverBoundaries(unittest.TestCase):
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


if __name__ == '__main__':
    unittest.main()
