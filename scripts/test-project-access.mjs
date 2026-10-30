import test from 'node:test';
import assert from 'node:assert/strict';
import {generateKeyPairSync,verify} from 'node:crypto';
import {mkdtempSync,writeFileSync,rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import {assertion,issuerOrigin,jsonRequest,sshArgs,checkOnline} from './project-access.mjs';

const {privateKey,publicKey}=generateKeyPairSync('rsa',{modulusLength:2048});
const key={clientId:'fixture-client',keyId:'fixture-key',key:privateKey.export({type:'pkcs8',format:'pem'})};
test('assertions bind the expected issuer/client and expire without leaking key material',()=>{
  const jwt=assertion(key,'https://identity.example',1000);const [header,claims,sig]=jwt.split('.');
  assert.equal(verify('RSA-SHA256',Buffer.from(header+'.'+claims),publicKey,Buffer.from(sig,'base64url')),true);
  assert.deepEqual(JSON.parse(Buffer.from(header,'base64url')),{alg:'RS256',typ:'JWT',kid:'fixture-key'});
  const p=JSON.parse(Buffer.from(claims,'base64url'));assert.equal(p.aud,'https://identity.example');assert.equal(p.iss,key.clientId);assert.equal(p.sub,key.clientId);assert.equal(p.exp,1060);assert.equal(p.iat,995);
  for(const origin of ['http://identity.example','https://user:password@identity.example','https://identity.example/other','https://identity.example/?x=1']) assert.throws(()=>issuerOrigin(origin));
});
test('HTTP checks forbid redirects, bound responses and suppress private error details',async()=>{
  let observed;
  const r=await jsonRequest('https://identity.example', {redirect:'follow'},async(url,opts)=>{observed=opts;throw Error('secret bearer reflected here');});
  assert.equal(observed.redirect,'error');assert.ok(observed.signal);assert.equal(JSON.stringify(r).includes('secret'),false);
  const large=await jsonRequest('https://identity.example',{},async()=>new Response('x'.repeat(131073)));
  assert.equal(large.error,'response_too_large');
});
test('SSH needs actual reference files, strict trust and an injection-safe target',()=>{
  const root=mkdtempSync(join(tmpdir(),'rroflow-ssh-check-'));
  try {
    const c={host:'example.test',user:'root',port:22,ssh_identity_file:'identity',trusted_known_hosts_file:'known_hosts'};
    assert.throws(()=>sshArgs(root,c));writeFileSync(join(root,'identity'),'fixture');writeFileSync(join(root,'known_hosts'),'fixture');
    const args=sshArgs(root,c);assert.ok(args.includes('StrictHostKeyChecking=yes'));assert.ok(args.includes('UpdateHostKeys=no'));assert.ok(args.includes('GlobalKnownHostsFile=none'));assert.deepEqual(args.slice(0,2),['-F','none']);
    assert.equal(args.at(-1),'root@example.test');assert.throws(()=>sshArgs(root,{...c,host:'-oProxyCommand=oops'}));assert.throws(()=>sshArgs(root,{...c,port:0}));
  } finally {rmSync(root,{recursive:true,force:true});}
});
test('online reports authenticate account tokens and Nomen without copying credentials or remote bodies',async()=>{
  const root=mkdtempSync(join(tmpdir(),'rroflow-access-check-'));
  try {
    writeFileSync(join(root,'token'),'fixture-secret-token');writeFileSync(join(root,'key.json'),JSON.stringify(key));
    const c={cloudflare:{zone:'example.test',token_file:'token'},nomen:{issuer:'https://identity.example',ingest_key_file:'key.json'}};
    const requests=[];let mismatch=false;
    const fetcher=async(url,opts)=>{
      requests.push({url,opts});assert.equal(opts.redirect,'error');let body;
      if(url.includes('/zones?'))body={result:[{id:'a'.repeat(32),name:'example.test',account:{id:'b'.repeat(32)},permissions:['#dns_records:edit']}]};
      else if(url.includes('/tokens/verify'))body={result:{status:'active'}};
      else if(url.includes('/dns_records?'))body={result:[]};
      else if(url.includes('/.well-known/'))body={issuer:mismatch?'https://other.example':c.nomen.issuer,introspection_endpoint:'https://other.example/steal'};
      else {
        assert.equal(url,'https://identity.example/oauth/v2/introspect');assert.equal(opts.method,'POST');assert.equal(opts.body.get('client_assertion').split('.').length,3);body={active:false,untrusted_secret:'must-not-appear'};
      }
      return Response.json(body);
    };
    const r=await checkOnline(root,c,fetcher);assert.equal(r.cloudflare.token_active,true);assert.equal(r.nomen.ingest_authentication_verified,true);assert.equal(r.nomen.member_signin_verified,false);
    assert.ok(requests.some(r=>r.url.includes('/accounts/'+ 'b'.repeat(32)+'/tokens/verify')));
    assert.equal(JSON.stringify(r).includes('fixture-secret-token'),false);assert.equal(JSON.stringify(r).includes('must-not-appear'),false);assert.equal(JSON.stringify(r).includes('PRIVATE KEY'),false);
    requests.length=0;mismatch=true;const rejected=await checkOnline(root,c,fetcher);assert.equal(rejected.nomen.issuer_matches,false);assert.equal(requests.some(r=>r.opts.method==='POST'),false);
  } finally {rmSync(root,{recursive:true,force:true});}
});
