/* SPDX-License-Identifier: GPL-3.0-or-later
 * Offer both official sign-in screens. The publisher owns all form inputs.
 */
(() => {
    'use strict';
    const current = new URL(location.href);
    if (current.origin !== 'https://mtacc.mobilelegends.com' || current.username || current.password) return;
    const base = '/v3.0/inapp/';
    const passwordForm = [base + 'login-new', base + 'login'].includes(location.pathname);
    const codeForm = location.pathname === base + 'loginbyemail';
    if (!passwordForm && !codeForm) {
        if (location.pathname !== '/' && !['/v3.0', '/v3.0/', '/v3.0/index.html'].includes(location.pathname)) return;
        const original = window.LoadPage;
        if (typeof original !== 'function' || original.clbbEmailPreference) return;
        let firstLogin = true;
        const preferred = function(page, ...rest) {
            if (firstLogin && (page === 'login-new' || page === 'login')) {
                firstLogin = false;
                return original.call(this, 'loginbyemail', ...rest);
            }
            return original.call(this, page, ...rest);
        };
        Object.defineProperty(preferred, 'clbbEmailPreference', {value: true});
        window.LoadPage = preferred;
        return;
    }

    if (!document.body || document.getElementById('clbb-login-method')) return;
    const russian = [document.documentElement.lang, navigator.language].some(lang => /^ru\b/i.test(lang));
    const bar = document.createElement('aside');
    bar.id = 'clbb-login-method';
    bar.style.cssText = 'position:fixed;inset:auto 0 max(4px,env(safe-area-inset-bottom)) 0;z-index:2147483647;display:flex;justify-content:center;align-items:center;gap:8px;flex-wrap:wrap;pointer-events:none;font:14px/1.3 sans-serif;text-align:center';
    const notice = document.createElement('span');
    notice.setAttribute('role', 'status');
    notice.style.cssText = 'color:white;background:#202632;border-radius:4px;padding:3px 8px';
    notice.hidden = true;
    const button = document.createElement('button');
    button.type = 'button';
    button.textContent = passwordForm
        ? (russian ? 'Войти по коду из письма' : 'Use an email code')
        : (russian ? 'Войти по паролю' : 'Use a password');
    button.style.cssText = 'pointer-events:auto;font:inherit;color:#fff;background:#243852;border:1px solid #aebed0;border-radius:5px;padding:5px 12px;cursor:pointer';
    button.addEventListener('click', () => {
        const target = new URL(location.href);
        target.pathname = base + (passwordForm ? 'loginbyemail' : 'login-new');
        location.replace(target.href);
    });
    bar.append(notice, button);
    document.body.appendChild(bar);
    if (passwordForm) window.addEventListener('clbb-password-unavailable', () => {
        notice.textContent = russian ? 'Сервис входа по паролю вернул HTTP 405.' : 'Password sign-in returned HTTP 405.';
        notice.hidden = false;
    });
})();
