import {HeimdallGlobe} from './vendor/heimdall-globe.js';
import {GlobeCamera} from './globe-camera.js';

function rgba(color){
  if(/^#[0-9a-f]{6}$/i.test(color))return [1,3,5].map(i=>parseInt(color.slice(i,i+2),16)/255).concat(1);
  const v=color.match(/[\d.]+/g)?.map(Number);
  return v?.length>=3?[v[0]/255,v[1]/255,v[2]/255,v[3]??1]:[.149,.231,.282,1];
}
function segments(points){const out=[];for(let i=2;i<points.length;i+=2){const p=[points[i-2],points[i-1],points[i],points[i+1]];if(p.every(Number.isFinite))out.push(p[0]/180,p[1]/90,p[2]/180,p[3]/90);}return new Float32Array(out);}

// Retains HEIMDALL's WebGL projector and hemisphere clipping. Iono supplies
// its established palette, legend, coastline, station dots and link callbacks.
export class PropagationGlobe extends HeimdallGlobe {
  constructor(canvas,onChange,onReset){
    super(canvas);this.camera=new GlobeCamera();this.entries=[];this.coastline=null;
    this.pathBuffer=this.gl.createBuffer();this.onChange=onChange;this.onReset=onReset;
    this.unbind=bindGlobeNavigation(canvas,this.camera,onChange,onReset);
  }
  uniforms(program){
    const gl=this.gl,aspect=this.canvas.clientWidth/Math.max(1,this.canvas.clientHeight),z=this.camera.zoom;
    gl.uniform2f(gl.getUniformLocation(program,'u_scale'),.9*z/Math.max(1,aspect),.9*z*Math.min(1,aspect));
    gl.uniform2f(gl.getUniformLocation(program,'u_rotation'),this.camera.yaw,this.camera.pitch);
    gl.uniform1f(gl.getUniformLocation(program,'u_globe'),1);
  }
  setCoastline(points){if(points===this.coastline)return;this.coastline=points;const data=segments(points);this.lineVertices=data.length/2;this.gl.bindBuffer(this.gl.ARRAY_BUFFER,this.lineBuffer);this.gl.bufferData(this.gl.ARRAY_BUFFER,data,this.gl.STATIC_DRAW);}
  setEntries(entries){this.entries=entries.map(e=>({...e,data:e.points?new Float32Array(Array.from(e.coords,(v,i)=>v/(i%2?90:180))):segments(e.coords)}));this.draw();}
  plot_rect(){return [0,0,this.canvas.clientWidth,this.canvas.clientHeight];}
  project_points(points){return this.camera.project(points,this.canvas.clientWidth,this.canvas.clientHeight);}
  draw(){
    if(!this.camera)return;
    super.draw();
    const gl=this.gl,p=this.lineProgram,a=gl.getAttribLocation(p,'a_position');
    gl.useProgram(p);this.uniforms(p);gl.enableVertexAttribArray(a);
    gl.bindBuffer(gl.ARRAY_BUFFER,this.pathBuffer);gl.vertexAttribPointer(a,2,gl.FLOAT,false,8,0);
    const rim=[];
    for(let i=0;i<=360;i+=2){const a=i*Math.PI/180,x=Math.sin(a),y=Math.cos(a),z=.0001,cp=Math.cos(this.camera.pitch),sp=Math.sin(this.camera.pitch);rim.push((Math.atan2(x,-y*sp+z*cp)-this.camera.yaw)*180/Math.PI,Math.asin(Math.max(-1,Math.min(1,y*cp+z*sp)))*180/Math.PI);}
    for(const e of [{data:segments(rim),color:'#d5dde2'},...this.entries]){
      gl.uniform4f(gl.getUniformLocation(p,'u_color'),...rgba(e.color));
      gl.uniform1f(gl.getUniformLocation(p,'u_points'),e.points?1:0);
      gl.uniform1f(gl.getUniformLocation(p,'u_point_size'),(e.points?6:1)*Math.min(2,devicePixelRatio||1));
      gl.bufferData(gl.ARRAY_BUFFER,e.data,gl.DYNAMIC_DRAW);
      // WebGL lineWidth is commonly fixed at one device pixel. Match Iono's
      // one-CSS-pixel links on HiDPI screens without a heavyweight renderer.
      const passes=e.points?1:Math.ceil(Math.min(2,devicePixelRatio||1));
      for(let x=0;x<passes;x++)for(let y=0;y<passes;y++){
        gl.viewport(x,y,this.canvas.width,this.canvas.height);
        gl.drawArrays(e.points?gl.POINTS:gl.LINES,0,e.data.length/2);
      }
      gl.viewport(0,0,this.canvas.width,this.canvas.height);
    }
  }
  free(){this.unbind();const gl=this.gl;for(const k of ['pathBuffer','meshBuffer','lineBuffer','terminatorBuffer','magneticBuffer','stationBuffer'])gl.deleteBuffer(this[k]);gl.deleteProgram(this.meshProgram);gl.deleteProgram(this.lineProgram);gl.getExtension('WEBGL_lose_context')?.loseContext();this.canvas.replaceWith(this.canvas.cloneNode(false));}
}

export function bindGlobeNavigation(canvas,camera,redraw,reset){
  const listeners=[],pointers=new Map();let moved=false,start=null;
  const on=(name,fn,options)=>{canvas.addEventListener(name,fn,options);listeners.push(()=>canvas.removeEventListener(name,fn,options));};
  const pair=()=>{const p=[...pointers.values()];return {x:(p[0].x+p[1].x)/2,y:(p[0].y+p[1].y)/2,d:Math.hypot(p[0].x-p[1].x,p[0].y-p[1].y)};};
  on('pointerdown',e=>{if(e.button!==0)return;canvas.setPointerCapture(e.pointerId);pointers.set(e.pointerId,{x:e.clientX,y:e.clientY});if(pointers.size===1){moved=false;start={x:e.clientX,y:e.clientY};}});
  on('pointermove',e=>{const old=pointers.get(e.pointerId);if(!old)return;const before=pointers.size>=2?pair():null;pointers.set(e.pointerId,{x:e.clientX,y:e.clientY});if(before){const after=pair();camera.zoomBy(after.d/Math.max(1,before.d));camera.rotate(after.x-before.x,after.y-before.y);moved=true;}else{camera.rotate(e.clientX-old.x,e.clientY-old.y);moved ||= Math.hypot(e.clientX-start.x,e.clientY-start.y)>4;}redraw();});
  for(const name of ['pointerup','pointercancel','lostpointercapture'])on(name,e=>{pointers.delete(e.pointerId);if(pointers.size===1)start=[...pointers.values()][0];});
  on('click',e=>{if(moved){e.stopImmediatePropagation();moved=false;}},true);
  on('wheel',e=>{if(!e.ctrlKey&&!e.metaKey)return;e.preventDefault();camera.zoomBy(Math.exp(-e.deltaY*.002));redraw();},{passive:false});
  on('dblclick',e=>{e.preventDefault();reset();});
  on('keydown',e=>{if(['+','=','-','ArrowLeft','ArrowRight','ArrowUp','ArrowDown','Home'].includes(e.key)){e.preventDefault();if(e.key==='Home')reset();else{if(e.key==='+'||e.key==='=')camera.zoomBy(1.4);else if(e.key==='-')camera.zoomBy(1/1.4);else camera.rotate(e.key==='ArrowLeft'?-12:e.key==='ArrowRight'?12:0,e.key==='ArrowUp'?-12:e.key==='ArrowDown'?12:0);redraw();}}});
  return ()=>{listeners.forEach(fn=>fn());pointers.clear();};
}
