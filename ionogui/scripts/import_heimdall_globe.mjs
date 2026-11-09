// Vendor the existing HEIMDALL GIM viewer, retaining its spherical WebGL core.
import {readFileSync,writeFileSync,mkdirSync} from 'node:fs';
import {stripTypeScriptTypes} from 'node:module';
const text=readFileSync(process.argv[2],'utf8');
let source=text.slice(text.indexOf('class WebglVtecMap{'),text.indexOf('\nconst plot=new WebglVtecMap'));
if(!source||!source.includes('void this.loadCoastlines()'))throw Error('Unrecognized HEIMDALL viewer');
source=source.replace('class WebglVtecMap','export class HeimdallGlobe');
source=source.replace('constructor(private canvas:HTMLCanvasElement){','constructor(canvas:HTMLCanvasElement){this.canvas=canvas;');
// Iono owns navigation/resize and supplies its existing coastline data.
source=source.replace(/new ResizeObserver[\s\S]*?void this\.loadCoastlines\(\)/,'this.globe=true');
source=source.replace('gl.clearColor(.02,.043,.075,1)','gl.clearColor(1,1,1,1)')
 .replace(".70,.86,.88,.88",'0.647,0.690,0.718,1')
 .replace('.20,1,.83,1','0.149,0.231,0.282,1');
mkdirSync('web/vendor',{recursive:true});
writeFileSync('web/vendor/heimdall-globe.js','// HEIMDALL GIM WebglVtecMap, jvierine/gim ab815d8a6881bb2f12de7356742c78ad4024a7ac.\n// Adapted for white-page Iono maps; provenance stays in source only.\n'+stripTypeScriptTypes(source));
