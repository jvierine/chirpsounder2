// CPU twin of the HEIMDALL GIM spherical shader, for exact hover/click targets.
export const globeProjection = url => new URL(url).searchParams.get('projection') === '2d' ? '2d' : '3d';
export class GlobeCamera {
  constructor(){this.reset('world');}
  reset(region){this.yaw=-(region==='norway'?17:15)*Math.PI/180;this.pitch=(region==='norway'?67:35)*Math.PI/180;this.zoom=region==='norway'?2.7:1;}
  rotate(dx,dy){this.yaw+=dx*.01/this.zoom;this.pitch=Math.max(-Math.PI/2,Math.min(Math.PI/2,this.pitch+dy*.01/this.zoom));}
  zoomBy(factor){this.zoom=Math.max(.5,Math.min(15,this.zoom*factor));}
  point(lon,lat){const l=lon*Math.PI/180+this.yaw,p=lat*Math.PI/180,c=Math.cos(p),y=Math.sin(p),z=c*Math.cos(l),cp=Math.cos(this.pitch),sp=Math.sin(this.pitch);return [c*Math.sin(l),y*cp-z*sp,y*sp+z*cp];}
  project(points,width,height){
    const out=[],radius=.45*Math.min(width,height)*this.zoom;
    const put=p=>out.push(width/2+p[0]*radius,height/2-p[1]*radius);
    let previous;
    for(let i=0;i<points.length;i+=2){
      if(!Number.isFinite(points[i])||!Number.isFinite(points[i+1])){out.push(NaN,NaN);previous=null;continue;}
      const p=this.point(points[i],points[i+1]);
      if(previous && (previous[2]>=0)!==(p[2]>=0)){
        const t=previous[2]/(previous[2]-p[2]),edge=previous.map((v,k)=>v+t*(p[k]-v));
        if(p[2]>=0)out.push(NaN,NaN);
        put(edge);
      }
      if(p[2]>=0)put(p);else out.push(NaN,NaN);
      previous=p;
    }
    return new Float64Array(out);
  }
}
