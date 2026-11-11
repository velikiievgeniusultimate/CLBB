// SPDX-License-Identifier: GPL-3.0-or-later
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const policy = fs.readFileSync(require('node:path').join(__dirname,'../distribution/moonton-email-login.js'),'utf8');
function setup(url) {
    const location=new URL(url), calls=[], navigations=[];
    location.replace=value=>navigations.push(value);
    const window={LoadPage:function(...args){calls.push({args,self:this});return 42;}};
    const context=vm.createContext({location,window,URL});
    vm.runInContext(policy,context);
    return {context,window,calls,navigations};
}
for(const path of ['/', '/v3.0/']) {
    const t=setup('https://mtacc.mobilelegends.com'+path);
    assert.equal(t.window.LoadPage('login-new','ru','private-fixture','device-fixture'),42);
    assert.deepEqual(Array.from(t.calls[0].args),['loginbyemail','ru','private-fixture','device-fixture']);
    assert.equal(t.calls[0].self,t.window);
    const first=t.window.LoadPage;vm.runInContext(policy,t.context);assert.equal(first,t.window.LoadPage);
    t.window.LoadPage('changebindemail');assert.equal(t.calls[1].args[0],'changebindemail');
}
for(const url of ['https://elsewhere.invalid/','https://mtacc.mobilelegends.com.evil.invalid/','http://mtacc.mobilelegends.com/','https://mtacc.mobilelegends.com/unrelated/']) {
    const t=setup(url);t.window.LoadPage('login-new');assert.equal(t.calls[0].args[0],'login-new');assert.equal(t.navigations.length,0);
}
const direct=setup('https://mtacc.mobilelegends.com/v3.0/inapp/login-new?fixture=1#keep');
assert.deepEqual(direct.navigations,['https://mtacc.mobilelegends.com/v3.0/inapp/loginbyemail?fixture=1#keep']);
assert.equal(setup('https://mtacc.mobilelegends.com/v3.0/inapp/loginbyemail').navigations.length,0);
console.log('EMAIL_LOGIN_POLICY_OK root dispatcher, direct navigation, origin isolation, args, idempotence');
