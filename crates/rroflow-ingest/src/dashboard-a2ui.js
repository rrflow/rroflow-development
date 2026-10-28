/* Local, declarative adapter for shippin.a2ui.v1. No remote transport or executable payloads. */
(() => {
  'use strict';
  const catalog = Object.freeze({
    schema_version:'shippin.a2ui.catalog.v1', catalog_id:'rroflow.workspace.v1', surface_id:'rroflow.workspace',
    targets:Object.freeze([
      Object.freeze({target_id:'workspace.views',label:'Workspace destinations',accepts:Object.freeze(['view.select'])}),
      Object.freeze({target_id:'workspace.guide',label:'Workspace guide',accepts:Object.freeze(['coachmark.show','focus.set'])}),
    ]),
  });
  const routes = new Set(['overview','activity','signals','sources','projects','library','datasets','learning','settings','research']);
  const plain = v => !!v && typeof v === 'object' && !Array.isArray(v);
  const exact = (v,keys) => plain(v) && Object.keys(v).length === keys.length && keys.every(k => Object.hasOwn(v,k));
  const text = (v,min,max) => typeof v === 'string' && v.length >= min && v.length <= max;
  function create(apply) {
    let last = -1;
    const seen = new Set();
    return {
      reset() { last = -1; seen.clear(); },
      receive(e) {
        const error = code => ({ok:false,code});
        if (!exact(e,['schema_version','interaction_id','surface_id','sequence','issued_at','actor','narration','action']) ||
            e.schema_version !== 'shippin.a2ui.v1' || e.surface_id !== catalog.surface_id ||
            !/^int_[a-z0-9][a-z0-9_-]{5,63}$/.test(e.interaction_id) || !Number.isSafeInteger(e.sequence) || e.sequence < 0 ||
            typeof e.issued_at !== 'string' || !/^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(?:\.\d+)?(?:Z|[+-]\d\d:\d\d)$/.test(e.issued_at) || !Number.isFinite(Date.parse(e.issued_at)) ||
            !exact(e.actor,['kind','id']) || e.actor.kind !== 'local-operator' || !/^[a-z][a-z0-9-]{1,63}$/.test(e.actor.id) ||
            !text(e.narration,1,280) || !plain(e.action)) return error('INVALID_ENVELOPE');
        const a = e.action, target = catalog.targets.find(t => t.target_id === a.target_id);
        if (!target) return error('UNKNOWN_TARGET');
        if (!target.accepts.includes(a.kind)) return error('UNSUPPORTED_ACTION');
        if (a.kind === 'view.select' && (!exact(a,['kind','target_id','view_id']) || !routes.has(a.view_id))) return error('INVALID_VALUE');
        if (a.kind === 'coachmark.show' && (!exact(a,['kind','target_id','title','body']) || !text(a.title,1,80) || !text(a.body,1,280))) return error('INVALID_VALUE');
        if (a.kind === 'focus.set' && !exact(a,['kind','target_id'])) return error('INVALID_VALUE');
        if (seen.has(e.interaction_id)) return error('DUPLICATE_INTERACTION');
        if (e.sequence <= last) return error('OUT_OF_ORDER');
        apply(Object.freeze({...a}));
        last = e.sequence; seen.add(e.interaction_id);
        return {ok:true};
      },
    };
  }
  window.RroflowA2UI = Object.freeze({catalog,create});
})();
