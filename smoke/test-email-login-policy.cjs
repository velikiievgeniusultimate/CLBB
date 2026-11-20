// SPDX-License-Identifier: GPL-3.0-or-later
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const policy = fs.readFileSync(require('node:path').join(__dirname, '../distribution/moonton-email-login.js'), 'utf8');
const origin = 'https://mtacc.mobilelegends.com';
const prefix = '/v3.0/inapp/';

function setup(url, language = 'en') {
    const location = new URL(url), calls = [], navigations = [], ids = new Map(), events = new Map();
    function element() {
        return {
            children: [], style: {}, events: new Map(),
            setAttribute(key, value) { this[key] = value; },
            addEventListener(key, callback) { this.events.set(key, callback); },
            append(...items) { this.children.push(...items); },
            appendChild(child) { this.append(child); if (child.id) ids.set(child.id, child); },
        };
    }
    const document = { body: element(), documentElement: {lang: language}, createElement: element, getElementById: id => ids.get(id) };
    location.replace = value => navigations.push(value);
    const window = {
        LoadPage(...args) { calls.push({args, self: this}); return 42; },
        addEventListener(key, callback) { events.set(key, callback); },
    };
    const context = vm.createContext({location, window, document, navigator: {language}, URL});
    vm.runInContext(policy, context);
    return {context, window, document, events, calls, navigations};
}

for (const path of ['/', '/v3.0/', '/v3.0/index.html']) {
    const t = setup(origin + path);
    const receiver = {};
    assert.equal(t.window.LoadPage.call(receiver,'login-new','ru','fixture-context','fixture-device'),42);
    assert.deepEqual(t.calls[0].args, ['loginbyemail','ru','fixture-context','fixture-device']);
    assert.equal(t.calls[0].self, receiver);
    const first = t.window.LoadPage;
    vm.runInContext(policy, t.context);
    assert.equal(first, t.window.LoadPage);
    t.window.LoadPage('changebindemail');
    assert.equal(t.calls[1].args[0], 'changebindemail');
    t.window.LoadPage('login-new');
    assert.equal(t.calls[2].args[0], 'login-new');
    assert.equal(t.document.body.children.length, 0);
}
{
    const t = setup(origin + '/');
    t.window.LoadPage('loginbyemail');
    t.window.LoadPage('login-new');
    assert.deepEqual(t.calls.map(call => call.args[0]), ['loginbyemail', 'login-new']);
}
for (const [page, destination] of [['login-new','loginbyemail'], ['login','loginbyemail'], ['loginbyemail','login-new']]) {
    const t = setup(origin + prefix + page + '?fixture=1#keep', 'ru-RU');
    const bar = t.document.body.children[0];
    const [notice, button] = bar.children;
    assert.equal(t.navigations.length, 0);
    assert.equal(notice.hidden, true);
    assert.equal(button.textContent, page === 'loginbyemail' ? 'Войти по паролю' : 'Войти по коду из письма');
    vm.runInContext(policy, t.context);
    assert.deepEqual(t.document.body.children, [bar]);
    if (page !== 'loginbyemail') {
        t.events.get('clbb-password-unavailable')();
        assert.equal(notice.hidden, false);
        assert.match(notice.textContent, /405/);
        assert.deepEqual(t.navigations, []);
    } else assert.equal(t.events.size, 0);
    button.events.get('click')();
    assert.deepEqual(t.navigations, [origin + prefix + destination + '?fixture=1#keep']);
}
for (const url of ['http://mtacc.mobilelegends.com/', 'https://mtacc.mobilelegends.com.evil.invalid/',
    'https://mtacc.mobilelegends.com:444'+prefix+'login', 'https://sample@mtacc.mobilelegends.com'+prefix+'login',
    origin+'/unrelated/', origin+prefix+'forgotpassword-email', origin+prefix+'login-newer']) {
    const t = setup(url);
    t.window.LoadPage('login-new');
    assert.equal(t.calls[0].args[0], 'login-new');
    assert.deepEqual(t.navigations, []);
    assert.deepEqual(t.document.body.children, []);
}
console.log('LOGIN_METHODS_OK initial preference, explicit password and code routes, 405 feedback, unrelated routes');
