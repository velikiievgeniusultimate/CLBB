// SPDX-License-Identifier: GPL-3.0-or-later
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const policy = fs.readFileSync(require('node:path').join(__dirname, '../distribution/moonton-email-login.js'), 'utf8');
const origin = 'https://mtacc.mobilelegends.com';
const prefix = '/v3.0/inapp/';

function setup(url, delayed = false) {
    const location = new URL(url), calls = [], navigations = [], events = {}, stored = new Map();
    location.replace = to => navigations.push(to);
    const original = function(...args) { calls.push({args, self:this}); return 42; };
    const window = {LoadPage: delayed ? undefined : original, addEventListener(type, cb) { events[type] = cb; }};
    const context = vm.createContext({location, window, document:{body:null}, URL,
        sessionStorage:{setItem:(k,v) => stored.set(k,v)}});
    vm.runInContext(policy, context);
    if (delayed) { window.LoadPage = original; events.load(); }
    return {window, context, calls, navigations, stored};
}

for (const path of ['/', '/v3.0', '/v3.0/', '/v3.0/index.html']) {
    const t = setup(origin + path, path === '/v3.0/');
    const receiver = {};
    for (const [page,expected] of [['login-new','loginbyemail'], ['loginbyemail','loginbyemail'],
        ['changebindemail','changebindemail'], ['login','loginbyemail'], ['forgotpassword-email','forgotpassword-email']]) {
        assert.equal(t.window.LoadPage.call(receiver,page,'ru','fixture-context',7),42);
        assert.deepEqual(t.calls.at(-1), {args:[expected,'ru','fixture-context',7], self:receiver});
    }
    const wrapper = t.window.LoadPage;
    vm.runInContext(policy, t.context);
    assert.equal(t.window.LoadPage, wrapper);
    assert.equal(t.stored.size, 0);
}
for (const page of ['login','login-new']) {
    const t = setup(origin + prefix + page + '?ctx=A%2FB&lang=ru#keep');
    assert.deepEqual(t.navigations, [origin + prefix + 'loginbyemail?ctx=A%2FB&lang=ru#keep']);
    assert.equal(t.stored.get('clbb-password-unavailable'), '1');
}
for (const url of ['http://mtacc.mobilelegends.com/', 'https://mtacc.mobilelegends.com.evil.invalid/',
    'https://mtacc.mobilelegends.com:444'+prefix+'login', 'https://sample@mtacc.mobilelegends.com'+prefix+'login',
    origin+'/unrelated/', origin+prefix+'forgotpassword-email', origin+prefix+'login-newer', origin+prefix+'loginbyemail']) {
    const t = setup(url);
    t.window.LoadPage('login-new');
    assert.equal(t.calls[0].args[0], 'login-new');
    assert.deepEqual(t.navigations, []);
    assert.equal(t.stored.size, 0);
}
console.log('LOGIN_POLICY_OK SDK context and receiver, repeated entry, direct routes, recovery and origin isolation');
