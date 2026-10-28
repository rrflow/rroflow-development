// Public page reads only public auth configuration; it never fetches member data.
(async()=>{
  const status=document.getElementById('member-status');
  try{
    const response=await fetch('/auth/config',{credentials:'omit'});
    if(!response.ok)throw new Error('unavailable');
    const config=await response.json();
    if(config.enabled && config.member_origin){
      const origin=new URL(config.member_origin);
      if(origin.protocol!=='https:' || origin.username || origin.password || origin.pathname!=='/' || origin.search || origin.hash)throw new Error('invalid origin');
      for(const link of document.querySelectorAll('[data-member-link]')){
        const target=new URL(link.getAttribute('href'),location.origin);
        link.href=origin.origin+'/app'+target.hash;
      }
      status.textContent='Continue to account sign-in. Your workspace opens after your identity is verified.';
    }else status.textContent='Member sign-in is not activated on this server yet. You can explore the product here.';
    // Keep existing bookmarked dashboard hashes working without accepting a redirect URL.
    if(/^#(overview|activity|signals|sources|projects|library|datasets|learning|settings|research)(\?|$)/.test(location.hash)){
      // Public sections own these anchors; only unambiguous legacy workspace links redirect.
      if(!['#learning'].includes(location.hash)) location.replace('/app'+location.hash);
    }
  }catch{
    status.textContent='Member access could not be checked. Open your workspace to retry.';
  }
})();
