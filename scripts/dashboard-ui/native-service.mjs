// Isolated native account service. Inspecting its encrypted outbox substitutes for
// mailbox delivery ONLY; the production API has no token-reading test endpoint.
import {mkdtemp,writeFile} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join,resolve} from 'node:path';
import {createServer} from 'node:http';
import {randomBytes,createDecipheriv,createHash} from 'node:crypto';
import {spawn} from 'node:child_process';
import pg from 'pg';
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
export async function nativeFixture(database,{device=false,resourceLoopback=false}={}) {
  const schema='native_ui_'+randomBytes(12).toString('hex');
  const admin=new pg.Client({connectionString:database});await admin.connect();
  await admin.query(`CREATE SCHEMA ${schema}`);
  const url=new URL(database);url.searchParams.set('options',`-csearch_path=${schema}`);
  const sql=new pg.Client({connectionString:url.toString()});await sql.connect();
  const root=await mkdtemp(join(tmpdir(),'rroflow-native-ui-'));
  const key=randomBytes(32),keyFile=join(root,'keys.json'),config=join(root,'service.toml');
  await writeFile(keyFile,JSON.stringify({active:'fixture',keys:{fixture:key.toString('hex')}}),{mode:0o600});
  const finder=createServer();await new Promise(r=>finder.listen(0,'127.0.0.1',r));const port=finder.address().port;await new Promise(r=>finder.close(r));
  const base=`http://127.0.0.1:${port}`,origin='https://native-account.example.test';
  const tomlPath=p=>JSON.stringify(p.replaceAll('\\','/'));
  await writeFile(config,`listen = "127.0.0.1:${port}"
[accounts]
origin = "${origin}"
encryption_keys_file = ${tomlPath(keyFile)}
signup_enabled = true
[accounts.smtp]
host = "smtp.example.test"
port = 587
from = "accounts@example.test"
credentials_file = ${tomlPath(join(root,'not-used.json'))}
${device ? `[accounts.device]
resource = "${resourceLoopback ? base : origin}"
[[accounts.device.clients]]
id = "fixture-collector"
name = "Fixture collector"
` : ''}`);
  const env={...process.env,RROFLOW_DATABASE_URL:url.toString()};delete env.RROFLOW_INGEST_CONFIG;
  const child=spawn(resolve(process.env.RROFLOW_UI_BINARY),['serve','--config',config],{env,windowsHide:true,stdio:['ignore','ignore','pipe']});
  let stderr='';child.stderr.on('data',chunk=>{stderr=(stderr+chunk).slice(-4000);});
  const exited=new Promise(r=>child.once('exit',r));
  const close=async()=>{child.kill();await Promise.race([exited,sleep(5000)]);await sql.end();await admin.query(`DROP SCHEMA ${schema} CASCADE`);await admin.end();};
  try {
    let ready=false;for(let i=0;i<200;i++){try{if((await fetch(base+'/readyz')).ok){ready=true;break;}}catch{}await sleep(50);}if(!ready)throw new Error('Native fixture did not become ready: '+stderr);
    return {origin,base,close,sql,
      async connect(page){
        // Browser stays on HTTPS and enforces Secure cookies. Requests are carried
        // to the real loopback Rust service as a TLS-terminating proxy would do.
        await page.route('**/*',async route=>{
          const u=new URL(route.request().url());
          if(u.origin!==origin)return route.abort();
          const response=await route.fetch({url:base+u.pathname+u.search,maxRedirects:0});
          await route.fulfill({response});
        });
      },
      async mailLink(email){
        const {rows}=await sql.query('SELECT a.*,o.key_id,o.nonce,o.ciphertext FROM native_actions a JOIN native_mail_outbox o USING(token_sha256) WHERE a.email=$1 ORDER BY a.created_at DESC LIMIT 1',[email]);
        if(rows.length!==1)throw new Error('Expected exactly one queued mailbox action');
        const r=rows[0],aad=JSON.stringify(['rroflow.native-mail/v1',r.context_sha256,r.token_sha256,r.email,r.purpose,Math.floor(r.expires_at.getTime()/1000),r.key_id]);
        const decipher=createDecipheriv('aes-256-gcm',key,r.nonce);decipher.setAAD(Buffer.from(aad));decipher.setAuthTag(r.ciphertext.subarray(-16));
        const token=Buffer.concat([decipher.update(r.ciphertext.subarray(0,-16)),decipher.final()]).toString();
        if(createHash('sha256').update(token).digest('hex')!==r.token_sha256)throw new Error('Action digest mismatch');
        return origin+'/app/account#'+token;
      }
    };
  } catch(e){await close();throw e;}
}
