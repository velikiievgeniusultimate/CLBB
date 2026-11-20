/* SPDX-License-Identifier: GPL-3.0-or-later
 * The official MOONTON form handles the address, verification code and submission.
 */
(() => {
    'use strict';
    const origin = 'https://mtacc.mobilelegends.com';
    const base = '/v3.0/inapp/';
    const code = base + 'loginbyemail';
    const current = new URL(location.href);
    const isPublisher = url => url.origin === origin && !url.username && !url.password;
    if (!isPublisher(current)) return;

    const passwordPage = name => name === 'login-new' || name === 'login';
    const passwordURL = url => isPublisher(url) && url.pathname.startsWith(base) && passwordPage(url.pathname.slice(base.length));
    if (['/', '/v3.0', '/v3.0/', '/v3.0/index.html'].includes(current.pathname)) {
        function preferCode() {
            const original = window.LoadPage;
            if (typeof original !== 'function' || original.clbbEmailPreference) return;
            function load(page, ...context) {
                return original.call(this, passwordPage(page) ? 'loginbyemail' : page, ...context);
            }
            Object.defineProperty(load, 'clbbEmailPreference', {value: true});
            window.LoadPage = load;
        }
        preferCode();
        window.addEventListener('load', preferCode, {once: true});
        return;
    }

    const noticeKey = 'clbb-password-unavailable';
    function returnToCode() {
        try { sessionStorage.setItem(noticeKey, '1'); } catch {}
        const url = new URL(location.href);
        url.pathname = code;
        location.replace(url.href);
    }
    if (passwordURL(current)) { returnToCode(); return; }
    if (current.pathname !== code || !document.body || document.getElementById('clbb-code-style')) return;

    const style = document.createElement('style');
    style.id = 'clbb-code-style';
    style.textContent = `
      #clbb-code-dialog { box-sizing:border-box; width:min(390px,90vw); padding:22px; border:1px solid #734458;
        border-radius:12px; color:#f8f2f5; background:#221b24; font:16px/1.45 system-ui,sans-serif; text-align:start; }
      #clbb-code-dialog::backdrop { background:rgba(15,9,16,.72); }
      #clbb-code-dialog h2 { margin:0 0 8px; color:inherit; font:700 20px/1.25 system-ui,sans-serif; }
      #clbb-code-dialog p { margin:0 0 16px; color:#e4d6de; font:inherit; }
      #clbb-code-dialog button { min-height:36px; border:1px solid #e86a96; border-radius:6px; padding:6px 15px;
        color:white; background:#74334d; cursor:pointer; font:600 14px system-ui,sans-serif; }
      #clbb-password-info { display:block; min-height:28px; margin:4px 0 0 auto; padding:2px 6px;
        border:0; color:#e4ecfc; background:transparent; text-decoration:underline; cursor:pointer; font:14px/1.4 system-ui,sans-serif; }`;
    document.head.append(style);

    function russian() {
        const form = document.querySelector('.inapp');
        if (form?.classList.contains('ru')) return true;
        if (form?.classList.contains('en')) return false;
        return /^ru\b/i.test(document.documentElement.lang || navigator.language);
    }
    const tr = (ru, en) => russian() ? ru : en;
    function notice(unavailable, trigger) {
        const existing = document.getElementById('clbb-code-dialog');
        if (existing?.open) return;
        existing?.remove();
        const dialog = document.createElement('dialog');
        dialog.id = 'clbb-code-dialog';
        dialog.setAttribute('aria-labelledby', 'clbb-code-title');
        const title = document.createElement('h2');
        title.id = 'clbb-code-title';
        title.textContent = unavailable ? tr('Вход по паролю в CLBB невозможен', 'Password sign-in is unavailable in CLBB')
            : tr('Сервис входа MOONTON недоступен', 'MOONTON sign-in is unavailable');
        const detail = document.createElement('p');
        detail.textContent = unavailable ? tr('Для входа используйте код из письма MOONTON.', 'Use the verification code sent to your email.')
            : tr('Сервис вернул HTTP 405. Попробуйте позже.', 'The service returned HTTP 405. Please try again later.');
        const close = document.createElement('button');
        close.type = 'button';
        close.textContent = tr('Понятно', 'Got it');
        close.addEventListener('click', () => dialog.close());
        dialog.addEventListener('close', () => {
            dialog.remove();
            if (trigger?.isConnected) trigger.focus({preventScroll:true});
        }, {once:true});
        dialog.append(title, detail, close);
        document.body.append(dialog);
        dialog.showModal();
        close.focus();
    }

    const passwordLabel = /^(?:(?:log\s*in|sign\s*in|login) (?:via |with |using |by )?(?:an? |the )?(?:account )?password|(?:войти|вход|авторизация) (?:через |с помощью |при помощи |по |с |используя )?(?:парол(?:ь|ю|ем|я))(?: аккаунта| уч[её]тной записи)?)$/iu;
    const labelMatches = value => passwordLabel.test((value || '').trim().replace(/[›»→❯]+$/u, '').trimEnd().replace(/\s+/gu, ' '));
    function switchControl(target) {
        for (let el = target?.closest?.('*'); el && el !== document.body; el = el.parentElement) {
            if (el.id === 'clbb-code-dialog') return null;
            if (el.id === 'clbb-password-info') return el;
            if (el.matches('img,svg,i') && [el.previousElementSibling, el.nextElementSibling].some(
                sibling => sibling && labelMatches(sibling.textContent))) return el.parentElement;
            if (el.matches('a[href]') && passwordURL(new URL(el.href, location.href))) return el;
            // In the publisher UI the caption and its arrow have the same click target.
            if (labelMatches(el.getAttribute('aria-label')) || labelMatches(el.textContent)) return el;
        }
        return null;
    }
    function guard(event) {
        if (event.type === 'keydown' && !['Enter', ' '].includes(event.key)) return;
        const control = switchControl(event.target);
        if (!control) return;
        event.preventDefault();
        event.stopImmediatePropagation();
        notice(true, control);
    }
    window.addEventListener('click', guard, true);
    window.addEventListener('keydown', guard, true);
    for (const type of ['pointerdown', 'pointerup', 'touchstart', 'touchend', 'mousedown', 'mouseup']) {
        window.addEventListener(type, event => {
            if (switchControl(event.target)) event.stopImmediatePropagation();
        }, {capture:true, passive:true});
    }
    window.addEventListener('clbb-login-unavailable', () => notice(false));

    let returning = false;
    try {
        returning = sessionStorage.getItem(noticeKey) === '1';
        sessionStorage.removeItem(noticeKey);
    } catch {}
    let redirected = false;
    function syncForm() {
        if (location.pathname !== code) return;
        // A publisher build can switch forms inside this document, keeping its URL.
        const password = document.querySelector('input[type=password]');
        if (password?.getClientRects().length && !document.querySelector('.send-but')?.getClientRects().length) {
            // Recover from an in-place switch once. If the code route itself serves a
            // password form twice, leave its close control available for a fresh entry.
            const form = password.closest('form, .bigwrapper') || password.parentElement;
            for (const field of form.querySelectorAll('input, button[type=submit], .login-but')) field.inert = true;
            if (!redirected) {
                redirected = true;
                if (returning) notice(true); else returnToCode();
            }
            return;
        }
        const login = document.querySelector('.botm-but.login-but');
        if (!login || !document.querySelector('.send-but')) return;
        let fallback = document.getElementById('clbb-password-info');
        const content = login.closest('.inapp') || login.parentElement;
        const native = [...content.querySelectorAll('a,button,span,p,div,label,[role=button]')].some(el =>
            el !== fallback && (labelMatches(el.getAttribute('aria-label')) ||
                (el.childElementCount < 3 && labelMatches(el.textContent))) &&
                !el.contains(fallback) && el.getClientRects().length);
        if (native) { fallback?.remove(); return; }
        if (!fallback) {
            fallback = document.createElement('button');
            fallback.type = 'button';
            fallback.id = 'clbb-password-info';
            login.after(fallback);
        }
        const caption = tr('Войти с паролем', 'Log in via account password');
        if (fallback.textContent !== caption) fallback.textContent = caption;
    }
    const observer = new MutationObserver(syncForm);
    observer.observe(document.body, {subtree:true, childList:true, characterData:true, attributes:true,
        attributeFilter:['class', 'type', 'hidden', 'href']});
    syncForm();
    if (returning) notice(true);
})();
