// Copyright (c) 灵动岛 contributors. MIT.
// DSH 的客户端插件协议；只同步主视图已读确认，不操作会话或回答询问。
window.__ModuleLoader__.load({id:'win-island-dsh-bridge',factory:()=>({
  name:'win-island-dsh-bridge',
  inject:['connection','sessions','uiSession'],
  apply(ctx){
    ctx.effect(()=>{
      let active=true,inFlight=false,pending=false;
      const controller=new AbortController();
      const publish=async()=>{
        if(!active)return;
        pending=false;
        if(document.visibilityState!=='visible'||!document.hasFocus())return;
        const list=ctx.sessions.list.getSnapshot();
        if(list?.phase!=='ready')return;
        const readSessionIds=Object.values(list.byId??{})
          .filter(row=>(row.retainedBy?.mainView??0)>0)
          .map(row=>row.id).filter(id=>typeof id==='string'&&/^[A-Za-z0-9_.-]{1,128}$/.test(id)).slice(0,64);
        if(!readSessionIds.length)return;
        inFlight=true;
        try{await ctx.connection.rpc.call('/api','win-island/viewed',{readSessionIds},controller.signal);}catch{}
        finally{inFlight=false;if(active&&pending)void publish();}
      };
      const schedule=()=>{if(!active)return;pending=true;if(!inFlight)void publish();};
      const stopList=ctx.sessions.list.subscribe(schedule);
      const stopStatus=ctx.uiSession.sessionStatus.subscribe(schedule);
      window.addEventListener('focus',schedule);
      document.addEventListener('visibilitychange',schedule);
      const timer=setInterval(schedule,1500);
      schedule();
      return ()=>{
        active=false;controller.abort();clearInterval(timer);stopList();stopStatus();
        window.removeEventListener('focus',schedule);
        document.removeEventListener('visibilitychange',schedule);
      };
    });
  },
})});
