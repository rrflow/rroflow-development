// Operator checks only. No provisioning, DNS writes, trust changes or token output.
import {readFileSync, existsSync} from 'node:fs';
import {resolve} from 'node:path';
import {pathToFileURL} from 'node:url';
import {randomUUID, sign} from 'node:crypto';
import {execFileSync} from 'node:child_process';

export function readAccess(project) {
  const root=resolve(project,'.secrets');
  const config=JSON.parse(readFileSync(resolve(root,'access-references.json'),'utf8').replace(/^\uFEFF/,''));
  if(config.schema!=='rroflow.local-access/v1') throw Error('Unsupported access reference schema');
  return {root,config};
}
function file(root,value) {
  return typeof value==='string' && value.length ? resolve(root,value) : null;
}
export function issuerOrigin(value) {
  const u=new URL(value);
  if(u.protocol!=='https:' || u.username || u.password || u.search || u.hash || u.pathname!=='/') throw Error('Issuer must be an HTTPS origin');
  return u.origin;
}
export function assertion(key,issuer,now=Math.floor(Date.now()/1000)) {
  if(!key.clientId || !key.keyId || typeof key.key!=='string') throw Error('Missing ingest key fields');
  const enc=x=>Buffer.from(JSON.stringify(x)).toString('base64url');
  const data=enc({alg:'RS256',typ:'JWT',kid:key.keyId})+'.'+enc({iss:key.clientId,sub:key.clientId,aud:issuerOrigin(issuer),iat:now-5,exp:now+60,jti:randomUUID()});
  return data+'.'+sign('RSA-SHA256',Buffer.from(data),key.key).toString('base64url');
}
export async function jsonRequest(url,options={},fetcher=fetch) {
  try {
    const r=await fetcher(url,{...options,redirect:'error',signal:AbortSignal.timeout(12000)});
    if(!r.body) return {http:r.status,body:{}};
    const reader=r.body.getReader(); const chunks=[]; let size=0;
    try {
      while(true){const {done,value}=await reader.read();if(done)break;size+=value.length;if(size>131072){await reader.cancel();return {http:r.status,error:'response_too_large',body:{}};}chunks.push(value);}
      return {http:r.status,body:JSON.parse(Buffer.concat(chunks).toString('utf8'))};
    } finally {reader.releaseLock();}
  } catch {return {http:null,error:'request_failed',body:{}};}
}
export function sshArgs(root,c) {
  const key=file(root,c.ssh_identity_file), hosts=file(root,c.trusted_known_hosts_file);
  if(!key || !hosts || !existsSync(key) || !existsSync(hosts)) throw Error('SSH identity and original trusted known_hosts file are required');
  if(!/^[a-zA-Z0-9][a-zA-Z0-9.-]*$/.test(c.host) || !/^[a-zA-Z0-9_][a-zA-Z0-9_.-]*$/.test(c.user)) throw Error('Invalid SSH target');
  if(!Number.isInteger(c.port) || c.port<1 || c.port>65535) throw Error('Invalid SSH port');
  return ['-F','none','-o','BatchMode=yes','-o','IdentitiesOnly=yes','-o','StrictHostKeyChecking=yes','-o','UpdateHostKeys=no','-o','GlobalKnownHostsFile=none','-o','UserKnownHostsFile='+hosts,'-o','ConnectTimeout=10','-i',key,'-p',String(c.port),c.user+'@'+c.host];
}
export async function checkOnline(root,c,fetcher=fetch) {
  const report={};
  try {
    const zoneName=c.cloudflare.zone;
    if(typeof zoneName!=='string' || !/^[a-z0-9][a-z0-9.-]+\.[a-z]{2,}$/.test(zoneName)) throw Error();
    const token=readFileSync(file(root,c.cloudflare.token_file),'utf8').trim();
    const headers={Authorization:'Bearer '+token};
    const req=path=>jsonRequest('https://api.cloudflare.com/client/v4'+path,{headers},fetcher);
    const z=await req('/zones?name='+encodeURIComponent(zoneName)+'&per_page=1');
    const zone=Array.isArray(z.body.result) ? z.body.result.find(v=>v.name===zoneName) : null;
    report.cloudflare={zone_http:z.http,zone_visible:!!zone,dns_edit_advertised:zone?.permissions?.includes('#dns_records:edit')===true};
    if(zone && /^[a-f0-9]{32}$/.test(zone.id) && /^[a-f0-9]{32}$/.test(zone.account?.id)) {
      // Account tokens need the account endpoint, not /user/tokens/verify.
      let v=await req('/accounts/'+zone.account.id+'/tokens/verify');
      if(v.http===401 || v.http===403 || v.http===404) v=await req('/user/tokens/verify');
      report.cloudflare.token_http=v.http;report.cloudflare.token_active=v.body.result?.status==='active';
      report.cloudflare.records=[];
      for(const name of [zoneName,'app.'+zoneName,'api.'+zoneName]) {
        const r=await req('/zones/'+zone.id+'/dns_records?name='+encodeURIComponent(name));
        report.cloudflare.records.push({name,http:r.http,records:Array.isArray(r.body.result) ? r.body.result.filter(x=>x.name===name).map(x=>({type:x.type,content:x.content,proxied:x.proxied})) : []});
      }
    }
  } catch {report.cloudflare={error:'credential_or_configuration_unavailable'};}
  try {
    const issuer=issuerOrigin(c.nomen.issuer);
    const discovery=await jsonRequest(issuer+'/.well-known/openid-configuration',{},fetcher);
    report.nomen={discovery_http:discovery.http,issuer_matches:discovery.body.issuer===issuer};
    if(report.nomen.issuer_matches) {
      const key=JSON.parse(readFileSync(file(root,c.nomen.ingest_key_file),'utf8').replace(/^\uFEFF/,''));
      const r=await jsonRequest(issuer+'/oauth/v2/introspect',{method:'POST',headers:{'Content-Type':'application/x-www-form-urlencoded'},body:new URLSearchParams({client_assertion_type:'urn:ietf:params:oauth:client-assertion-type:jwt-bearer',client_assertion:assertion(key,issuer),token:'rroflow-readiness-inactive-'+randomUUID()})},fetcher);
      report.nomen.authentication_http=r.http;report.nomen.ingest_authentication_verified=r.http===200 && r.body.active===false;
    }
    report.nomen.member_signin_verified=false;
  } catch {report.nomen={error:'credential_or_configuration_unavailable',member_signin_verified:false};}
  return report;
}
export async function run(argv) {
  const args=[...argv], projectAt=args.indexOf('--project');
  if(projectAt<0 || !args[projectAt+1]) throw Error('Use --project <checkout> [--online] [--inspect-host]');
  const project=args.splice(projectAt,2)[1];
  if(args.some(a=>!['--online','--inspect-host'].includes(a))) throw Error('Unknown argument');
  const {root,config:c}=readAccess(project);
  const report={schema:'rroflow.access-check/v1',observed_at:new Date().toISOString(),tools:{},ssh:{configured:false,authenticated:false},member_service_verified:false};
  for(const [tool,version] of [['node','--version'],['gh','--version'],['ssh','-V']]) {
    try {execFileSync(tool,[version],{stdio:'pipe',timeout:5000,windowsHide:true});report.tools[tool]=true;}catch{report.tools[tool]=false;}
  }
  let ssh;
  try {ssh=sshArgs(root,c.texas);report.ssh.configured=true;}catch{report.ssh.blocker='Missing SSH identity or original trusted host file, or invalid target configuration';}
  if(args.includes('--online')) Object.assign(report,await checkOnline(root,c));
  if(args.includes('--inspect-host') && ssh) {
    const command='hostname; uname -sm; systemd-detect-virt; if test -d /etc/pve; then printf "PROXMOX_HOST=yes\\n"; else printf "PROXMOX_HOST=no\\n"; fi; df -h /; free -m; systemctl is-active rroflow-ingest postgresql caddy; true';
    try {report.ssh.inventory=execFileSync('ssh',[...ssh,command],{encoding:'utf8',timeout:25000,maxBuffer:131072,windowsHide:true});report.ssh.authenticated=true;}
    catch {report.ssh.blocker='Strict SSH authentication/inventory failed; no trust bypass or remote mutation attempted';}
  }
  return report;
}
if(process.argv[1] && import.meta.url===pathToFileURL(resolve(process.argv[1])).href) {
  try {
    const report=await run(process.argv.slice(2));console.log(JSON.stringify(report,null,2));
    // This tool reports access prerequisites, never a live-service readiness certificate.
    if(!report.ssh.configured || (process.argv.includes('--inspect-host')&&!report.ssh.authenticated) || (process.argv.includes('--online')&&(!report.nomen?.ingest_authentication_verified || !report.cloudflare?.zone_visible || !report.cloudflare?.token_active))) process.exitCode=2;
  } catch {console.error('Access check could not run. Check arguments and private access references; credential details suppressed.');process.exitCode=1;}
}
