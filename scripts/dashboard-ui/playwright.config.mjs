import {defineConfig} from '@playwright/test';
export default defineConfig({
  testDir:'.', testMatch:'dashboard.spec.mjs', workers:1, retries:0,
  timeout:60000, globalTimeout:600000,
  reporter:[['list'],['html',{open:'never'}]],
  use:{browserName:'chromium',trace:'retain-on-failure',screenshot:'only-on-failure',colorScheme:'light',reducedMotion:'reduce',locale:'en-US',timezoneId:'UTC'},
  projects:[
    {name:'desktop',use:{viewport:{width:1440,height:1000}}},
    {name:'phone',use:{viewport:{width:390,height:844},isMobile:true,hasTouch:true,deviceScaleFactor:1}},
  ],
});
