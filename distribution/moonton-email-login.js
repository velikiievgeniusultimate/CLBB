/* SPDX-License-Identifier: GPL-3.0-or-later
 * Select the publisher's existing email-code screen; never submit a form.
 */
(() => {
    'use strict';
    if (location.protocol !== 'https:' || location.hostname !== 'mtacc.mobilelegends.com') return;
    if (location.pathname !== '/' && !location.pathname.startsWith('/v3.0/')) return;
    const original = window.LoadPage;
    if (typeof original === 'function' && !original.clbbEmailPreference) {
        const preferred = function(page, ...rest) {
            return original.call(this, page === 'login-new' || page === 'login' ? 'loginbyemail' : page, ...rest);
        };
        Object.defineProperty(preferred, 'clbbEmailPreference', {value: true});
        window.LoadPage = preferred;
    }
    if (location.pathname === '/v3.0/inapp/login-new' || location.pathname === '/v3.0/inapp/login') {
        const target = new URL(location.href);
        target.pathname = '/v3.0/inapp/loginbyemail';
        location.replace(target.href);
    }
})();
