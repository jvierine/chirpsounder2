/** Pan with one pointer; pinch and move the midpoint with two touch pointers.
 * Desktop modifier-wheel and box zoom retain their existing behavior. */
export function bindPlotNavigation(canvas, viewport, redraw, axes = () => ({x:0,y:0,width:canvas.clientWidth,height:canvas.clientHeight})) {
 const box=document.createElement('div');
 Object.assign(box.style,{position:'fixed',pointerEvents:'none',display:'none',border:'1.5px dashed #176fa3',background:'rgba(45,137,200,.16)',boxSizing:'border-box',zIndex:'2147483647'});
 box.setAttribute('aria-hidden','true');document.body.append(box);
 const touchAction=canvas.style.touchAction; canvas.style.touchAction='none';
 const events=[];const on=(target,name,fn,options)=>{target.addEventListener(name,fn,options);events.push(()=>target.removeEventListener(name,fn,options));};
 const point=e=>{const r=canvas.getBoundingClientRect(),a=axes();return [(e.clientX-r.left-a.x)/a.width,1-(e.clientY-r.top-a.y)/a.height]};
 const clamp=p=>p.map(v=>Math.max(0,Math.min(1,v)));
 const touches=new Map(); let drag=null,pinch=null,moved=false;
 const pair=()=>{const [a,b]=[...touches.values()],r=axes();return {center:[(a[0]+b[0])/2,(a[1]+b[1])/2],distance:Math.hypot((a[0]-b[0])*r.width,(a[1]-b[1])*r.height)}};
 const cancel=()=>{drag=null;pinch=null;touches.clear();box.style.display='none'};
 const show=(start,end)=>{const r=canvas.getBoundingClientRect(),a=axes();Object.assign(box.style,{display:'block',left:`${r.left+a.x+Math.min(start[0],end[0])*a.width}px`,top:`${r.top+a.y+(1-Math.max(start[1],end[1]))*a.height}px`,width:`${Math.abs(end[0]-start[0])*a.width}px`,height:`${Math.abs(end[1]-start[1])*a.height}px`})};
 on(canvas,'wheel',e=>{if(!e.metaKey&&!e.ctrlKey)return;const p=point(e);if(p.some(v=>v<0||v>1))return;e.preventDefault();viewport.zoom(p[0],p[1],Math.exp(Math.max(-1,Math.min(1,e.deltaY*.002))));redraw()},{passive:false});
 on(canvas,'contextmenu',e=>{e.preventDefault();cancel();viewport.reset();redraw()});
 on(canvas,'pointerdown',e=>{
  if(e.button!==0)return;const p=point(e);if(p.some(v=>v<0||v>1))return;
  e.preventDefault();canvas.setPointerCapture(e.pointerId);
  if(e.pointerType==='touch') {
   if(touches.size===0)moved=false;
   touches.set(e.pointerId,p);
   if(touches.size>=2){pinch=pair();drag=null;moved=true;box.style.display='none';return;}
  } else moved=false;
  drag={id:e.pointerId,start:p,last:p,box:e.pointerType!=='touch'&&(e.metaKey||e.ctrlKey)};
  if(drag.box)show(p,p);
 });
 on(canvas,'pointermove',e=>{
  if(touches.has(e.pointerId))touches.set(e.pointerId,point(e));
  if(touches.size>=2){
   const next=pair();
   if(pinch&&pinch.distance>1&&next.distance>1){
    viewport.zoom(...pinch.center,pinch.distance/next.distance);
    viewport.pan(pinch.center[0]-next.center[0],pinch.center[1]-next.center[1]);redraw();
   }
   pinch=next;return;
  }
  if(!drag||drag.id!==e.pointerId)return;
  const p=clamp(point(e)),a=axes();if(Math.hypot((p[0]-drag.start[0])*a.width,(p[1]-drag.start[1])*a.height)>4)moved=true;
  if(drag.box)show(drag.start,p);else{viewport.pan(drag.last[0]-p[0],drag.last[1]-p[1]);redraw()}drag.last=p;
 });
 const end=e=>{
  touches.delete(e.pointerId);
  if(pinch){
   if(touches.size>=2){pinch=pair();return;}
   pinch=null; const one=touches.entries().next().value;
   drag=one?{id:one[0],start:one[1],last:one[1],box:false}:null;return;
  }
  if(!drag||drag.id!==e.pointerId)return;
  const p=clamp(point(e)),a=axes();
  if(e.type==='pointerup'&&drag.box&&Math.abs(p[0]-drag.start[0])*a.width>=4&&Math.abs(p[1]-drag.start[1])*a.height>=4){viewport.zoom_box(...drag.start,...p);moved=true;redraw()}
  drag=null;box.style.display='none';
 };
 on(canvas,'pointerup',end);on(canvas,'pointercancel',end);on(canvas,'lostpointercapture',end);
 on(window,'blur',cancel);on(window,'keydown',e=>{if(e.key==='Escape')cancel()});
 on(canvas,'click',e=>{if(moved){e.stopImmediatePropagation();moved=false}},true);
 return ()=>{events.forEach(remove=>remove());canvas.style.touchAction=touchAction;box.remove()};
}

/** Shared colorbar limit editor. Hit testing and scale application use the plot API.
 * Options: automaticLimits() -> [low,high], onChange(low,high,automatic).
 * The canvas may ignore pointer events; its parent still owns the colorbar hit region.
 */
export function bindColorbarEditor(canvas, plot, options = {}) {
  const host=canvas.parentElement||canvas;
  let dialog;
  const remove=()=>{dialog?.remove();dialog=null;};
  const open=event=>{
    const bounds=canvas.getBoundingClientRect();
    const x=event.clientX-bounds.left,y=event.clientY-bounds.top;
    const [left,top,width,height]=plot.colorbar_rect();
    if(x<left||x>left+width||y<top||y>top+height)return;
    event.preventDefault();event.stopPropagation();remove();
    dialog=document.createElement('dialog');
    Object.assign(dialog.style,{font:'16px system-ui',padding:'24px',border:'1px solid #aebfc6',borderRadius:'9px',width:'340px',maxWidth:'90vw',color:'#243746'});
    dialog.setAttribute('aria-label','Color limits');
    const heading=document.createElement('h2');heading.textContent='Color limits';heading.style.margin='0 0 10px';
    const unit=document.createElement('p');unit.textContent=plot.color_label();
    const inputs=Array.from(plot.scale_limits(),(value,i)=>{const label=document.createElement('label');label.textContent=i?'Upper limit':'Lower limit';label.style.display='block';label.style.margin='12px 0';const input=document.createElement('input');input.type='number';input.step='any';input.value=Number(value.toPrecision(7));input.setAttribute('aria-label',label.textContent);Object.assign(input.style,{font:'inherit',display:'block',width:'100%',padding:'8px',marginTop:'4px',boxSizing:'border-box'});label.append(input);return {label,input};});
    const error=document.createElement('p');error.setAttribute('role','alert');error.style.color='#a31e35';
    const actions=document.createElement('div');Object.assign(actions.style,{display:'flex',gap:'8px',justifyContent:'flex-end',marginTop:'18px'});
    const button=(text,fn)=>{const b=document.createElement('button');b.textContent=text;Object.assign(b.style,{font:'inherit',padding:'7px 10px',cursor:'pointer'});b.onclick=fn;actions.append(b);};
    button('Cancel',remove);
    if(options.automaticLimits)button('Automatic',()=>{const [a,b]=options.automaticLimits();plot.set_scale(a,b);options.onChange?.(a,b,true);remove();});
    button('Apply',()=>{const a=Number(inputs[0].input.value),b=Number(inputs[1].input.value);if(!inputs.every(p=>p.input.value.trim())||!Number.isFinite(a)||!Number.isFinite(b)||b<=a){error.textContent='Enter a lower limit smaller than the upper limit.';return;}plot.set_scale(a,b);options.onChange?.(a,b,false);remove();});
    dialog.append(heading,unit,...inputs.map(p=>p.label),error,actions);document.body.append(dialog);dialog.addEventListener('cancel',remove,{once:true});dialog.showModal();inputs[0].input.focus();inputs[0].input.select();
  };
  host.addEventListener('pointerdown',open,true);
  return ()=>{host.removeEventListener('pointerdown',open,true);remove();};
}
