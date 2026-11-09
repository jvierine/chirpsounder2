import {readArray} from "./arrays.js";
import {bindPlotNavigation,bindColorbarEditor} from "./navigation.js";
import {pathSourceLabel} from "./map-links.js?v=20261106-rothr-candidates";

export function linkQuality(row) {
  if(row.association_ambiguous) return {excluded:false,limited:true,commissioning:true,label:"Unresolved ROTHR source: this propagation-range gate overlaps another transmitter site. These are candidate echoes; quantitative frequency limits are withheld."};
  const crossModulated=row.receiver==="W2NAF" && ["Australia","Cyprus","JORN","NIC"].includes(row.transmitter);
  const commissioning=!crossModulated && row.measurement_quality==="commissioning";
  const excluded=crossModulated || (row.quantitative_eligible===false && !commissioning);
  const limited=row.receiver==="W2NAF" && (row.transmitter==="US" || row.transmitter.startsWith("ROTHR"));
  return {excluded,limited,commissioning,label:crossModulated?"Excluded from quantitative use: substantial cross-modulation at W2NAF.":commissioning?"Commissioning: ionograms and RTIs are available; automatic usable-frequency limits are withheld.":excluded?"Excluded from quantitative use.":limited?"Limited reliability: W2NAF–ROTHR measurements are somewhat reliable; source association remains provisional.":row.assumed?"Assumed source association · experimental measurements":"Experimental echo measurements"};
}
export function remapRows(p,first,block,end) {
  const step=120, nx=1440, result=new Float32Array(block.length/p.nx*nx).fill(NaN);
  const columns=p.x.map(t=>Math.floor((t-(end-172800))/step));
  for(let row=0;row<block.length/p.nx;row++) for(let x=0;x<p.nx;x++) {
    const j=columns[x]; if(j>=0 && j<nx) result[row*nx+j]=block[row*p.nx+x];
  }
  return result;
}
export function linkHistoryPopup({HeatmapView,PlotViewport,cache,json,fontSize,stamp,openIonogram}) {
  let session;
  function close() {
    const s=session;if(!s)return;session=null;s.abort.abort();
    for(const p of s.panels) {p.resize?.disconnect();p.unbind?.();p.uncolor?.();p.renderer?.free();p.viewport?.free();}
    s.el.remove();s.focus?.focus({preventScroll:true});
  }
  async function open(row,day,at=null) {
    close();
    const el=document.createElement("dialog");el.className="ionogram-popup link-history-popup";
    el.setAttribute("aria-label",`${pathSourceLabel(row)} to ${row.receiver} 48-hour link history`);
    const head=document.createElement("div"),title=document.createElement("h2"),exit=document.createElement("button");
    head.className="dialog-head";title.textContent=`${pathSourceLabel(row)} → ${row.receiver} · 48 hours`;
    exit.textContent="×";exit.setAttribute("aria-label","Close link history");exit.onclick=close;head.append(title,exit);
    const dates=document.createElement("p"),quality=document.createElement("p"),status=document.createElement("p"),progress=document.createElement("progress"),actions=document.createElement("div");
    const end=at??Date.now()/1000;dates.textContent=`${stamp(end-172800).replace(" UTC","")} – ${stamp(end)}`;
    const q=linkQuality(row);quality.className="link-quality";quality.dataset.quality=q.excluded?"excluded":q.limited?"limited":"experimental";quality.textContent=q.label;
    status.setAttribute("role","status");status.textContent="Loading receiver-prepared link data…";
    progress.max=2;progress.value=0;actions.className="link-actions";
    const latest=document.createElement("button");latest.textContent="Latest ionogram";latest.hidden=!!row.assumed;latest.onclick=()=>{close();openIonogram(row,day,at,()=>open(row,day,end));};actions.append(latest);
    el.append(head,dates,quality,actions,status,progress);
    const s={el,abort:new AbortController(),panels:[],focus:document.activeElement};session=s;
    document.body.append(el);el.show();exit.focus({preventScroll:true});
    el.addEventListener("keydown",e=>{if(e.key==="Escape"){e.preventDefault();close();}});
    const current=()=>session===s&&!s.abort.signal.aborted;
    async function load() {
      try {
        const response=await json(`api/day?day=${encodeURIComponent(day)}&station=${encodeURIComponent(row.receiver)}`,s.abort.signal);
        if(!current())return;
        const selected=["peak-frequency","range-frequency"].map(quantity=>response.products?.find(p=>p.kind==="link-history" && p.transmitter===row.transmitter && p.quantity===quantity));
        if(selected.some(p=>!p)){status.textContent="The receiver has not published 48-hour arrays for this link and day yet.";progress.hidden=true;return;}
        let coverageStart=Infinity,coverageEnd=-Infinity,observed=0;
        for(const [index,p] of selected.entries()) {
          if(!current())return;
          const container=document.createElement("div"),field=document.createElement("canvas"),axes=document.createElement("canvas");
          container.className="plot";field.className="field";axes.className="axes";container.append(field,axes);el.append(container);
          const renderer=new HeatmapView(field,axes),viewport=new PlotViewport(),panel={renderer,viewport};s.panels.push(panel);
          renderer.set_font_size(fontSize());renderer.set_palette("turbo");renderer.set_markers(false);renderer.set_reduction("nearest");renderer.set_x_labels_visible(index===1);
          renderer.set_labels("Time (UTC)",p.ylabel,p.color_label,true);renderer.set_scale(p.low,p.high);
          const dy=p.y[1]-p.y[0],y0=p.y[0]-dy/2,y1=p.y.at(-1)+dy/2;
          renderer.set_data(1440,p.ny,new Float32Array(1440*p.ny).fill(NaN),end-172800,end,y0,y1);
          const resize=()=>{if(!current())return;const r=container.getBoundingClientRect();renderer.resize(r.width,r.height,Math.min(1.5,devicePixelRatio||1));const [l,t,w,h]=renderer.plot_rect();Object.assign(field.style,{left:`${l}px`,top:`${t}px`,width:`${w}px`,height:`${h}px`});};
          resize();panel.resize=new ResizeObserver(resize);panel.resize.observe(container);
          const redraw=()=>{const [x0,y0,x1,y1]=viewport.bounds();for(const other of s.panels){const b=other.viewport.bounds();other.viewport.set_bounds(x0,other===panel?y0:b[1],x1,other===panel?y1:b[3]);other.renderer.set_view(...other.viewport.bounds());}};
          panel.unbind=bindPlotNavigation(field,viewport,redraw);field.ondblclick=()=>{for(const other of s.panels){other.viewport.reset();other.renderer.set_view(...other.viewport.bounds());}};
          panel.uncolor=bindColorbarEditor(axes,renderer,{automaticLimits:()=>[p.low,p.high]});
          const rows=(first,block)=>{if(!current())return;const mapped=remapRows(p,first,block,end);renderer.update_rows(first,mapped);if(index===0)for(let i=0;i<mapped.length;i++)if(Number.isFinite(mapped[i])){const time=end-172800+(i%1440+.5)*120;coverageStart=Math.min(coverageStart,time);coverageEnd=Math.max(coverageEnd,time);observed++;}};
          const cached=cache.get(p.id);
          if(cached)rows(0,cached);
          else {const data=await fetch(`api/data/${p.id}`,{signal:s.abort.signal,cache:"force-cache"});const values=await readArray(data,p.nx,p.ny,{signal:s.abort.signal,onRows:rows,onProgress:f=>{if(current())progress.value=index+f;}});if(!current())return;cache.set(p.id,values);}
          // Finite-cell marks keep narrow echoes visible when a full 48 hours
          // is compressed onto a screen. Enable only after row streaming ends.
          renderer.set_marker_size(1.25);renderer.set_markers(true);
          progress.value=index+1;
        }
        if(current()){progress.hidden=true;status.textContent=observed?`Data coverage: ${stamp(coverageStart)} – ${stamp(coverageEnd)} · White gaps contain no published measurements.`:"No detections in this link’s range interval during the selected 48 hours.";}
      } catch(error){if(current()){status.textContent=`Link data unavailable: ${error.message||error}`;progress.hidden=true;}}
    }
    if(q.excluded) {
      progress.hidden=true;status.textContent="Diagnostic plots are hidden by default and must not be interpreted quantitatively.";
      const show=document.createElement("button");show.textContent="Show excluded diagnostic data";show.onclick=()=>{show.remove();progress.hidden=false;status.textContent="Loading excluded diagnostic data…";load();};actions.append(show);
    } else load();
  }
  return {open,close};
}
