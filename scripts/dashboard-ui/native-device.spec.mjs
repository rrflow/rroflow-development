import {test,expect} from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import {randomBytes,createHash} from 'node:crypto';
import {nativeFixture} from './native-service.mjs';

test('member reviews a collector code, approves only a project, and disconnects it',async({page},info)=>{
  test.skip(!process.env.RROFLOW_DATASET_UI_DATABASE_URL,'Requires disposable Postgres');
  const f=await nativeFixture(process.env.RROFLOW_DATASET_UI_DATABASE_URL,{device:true});
  try{
    // Native account creation is setup; the existing account test covers its UI.
    const email='device-member@example.test',password='a private device fixture password';
    const json=async(path,body)=>fetch(f.base+path,{method:'POST',headers:{origin:f.origin,'x-rroflow-request':'dashboard','content-type':'application/json'},body:JSON.stringify(body)});
    expect((await json('/auth/native/request',{email,purpose:'register'})).status).toBe(202);
    const token=(await f.mailLink(email)).split('#')[1];
    expect((await json('/auth/native/complete',{token,password})).status).toBe(200);
    await f.connect(page);await page.goto(f.origin+'/app/device');
    await page.getByRole('link',{name:'Sign in to connect'}).click();
    await page.getByLabel('Email address').fill(email);await page.getByLabel('Password',{exact:true}).fill(password);
    await page.getByRole('button',{name:'Sign in',exact:true}).click();
    await expect(page).toHaveURL(f.origin+'/app/device');
    await expect(page.getByRole('heading',{name:'Enter the code from your terminal'})).toBeVisible();
    const form=async(path,values)=>fetch(f.base+'/auth/native/device/'+path,{method:'POST',body:new URLSearchParams(values)});
    const deviceName='Laptop <img src=x onerror=alert(1)>';
    const started=await form('start',{client_id:'fixture-collector',resource:f.origin,device_name:deviceName,scope:'capture'});
    expect(started.status).toBe(200);const authorization=await started.json();
    await page.getByLabel('Connection code').fill(authorization.user_code);
    await page.getByRole('button',{name:'Review connection',exact:true}).click();
    await expect(page.getByRole('heading',{name:'Review this connection'})).toBeVisible();
    await expect(page.getByText(deviceName,{exact:true})).toBeVisible();
    await expect(page.getByText(email,{exact:true})).toBeVisible();
    await expect(page.locator('img')).toHaveCount(0);
    await expect(page.getByLabel('Send activity to').locator('option:checked')).toHaveText('Personal / Default project');
    expect((await new AxeBuilder({page}).withTags(['wcag2a','wcag2aa','wcag21a','wcag21aa']).analyze()).violations).toEqual([]);
    expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth+1)).toBe(true);
    const proof=info.outputPath('device-approval-test-fixture.png');await page.screenshot({path:proof,fullPage:true});await info.attach('Collector approval (fixture)',{path:proof,contentType:'image/png'});
    await page.getByRole('button',{name:'Approve collector',exact:true}).click();
    await expect(page.getByRole('status')).toContainText('Approval saved');
    // Avoid real polling delay in this browser test; protocol timing is tested in Rust.
    await f.sql.query("UPDATE native_device_requests SET next_poll_at=clock_timestamp()-interval '1 second' WHERE device_sha256=$1",[createHash('sha256').update(authorization.device_code).digest('hex')]);
    const issued=await form('token',{client_id:'fixture-collector',grant_type:'urn:ietf:params:oauth:grant-type:device_code',device_code:authorization.device_code,request_id:randomBytes(32).toString('hex')});
    expect(issued.status).toBe(200);const credentials=await issued.json();
    const who=()=>fetch(f.base+'/v1/whoami',{headers:{authorization:'Bearer '+credentials.access_token}});
    expect((await who()).status).toBe(200);
    await page.getByRole('button',{name:'Refresh collectors',exact:true}).click();
    await expect(page.getByRole('button',{name:'Disconnect '+deviceName,exact:true})).toBeVisible();
    const storage=await page.evaluate(()=>JSON.stringify({...localStorage,...sessionStorage}));
    expect(storage).not.toContain('rrnc_');expect(storage).not.toContain('rrnr_');
    await page.getByRole('button',{name:'Disconnect '+deviceName,exact:true}).click();
    await expect(page.getByRole('dialog')).toBeVisible();
    expect((await new AxeBuilder({page}).withTags(['wcag2a','wcag2aa','wcag21a','wcag21aa']).analyze()).violations).toEqual([]);
    await page.getByRole('button',{name:'Disconnect collector',exact:true}).click();
    await expect(page.getByRole('status')).toHaveText('Collector disconnected.');
    await expect(page.getByText('No active collector approvals for this email account.')).toBeVisible();
    expect((await who()).status).toBe(401);
  }finally{await f.close();}
});
