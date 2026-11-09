import {test} from "node:test";
import assert from "node:assert/strict";
import {viewFromUrl, urlForView, mapRegion, urlForMapRegion, readPlotOrderCookie, plotOrderCookie, mergePlotOrder} from "../web/preferences.js";
import {groupProducts} from "../web/groups.js";

test("a shared URL overrides the browser's last station", () => {
  assert.equal(viewFromUrl("https://juha.no/iono2/?view=propagation", "DOB"), "propagation");
  assert.equal(viewFromUrl("https://juha.no/iono2/?view=kho", "DOB"), "KHO");
  assert.equal(viewFromUrl("https://juha.no/iono2/?view=krea", "DOB"), "KREA");
  assert.equal(viewFromUrl("https://juha.no/iono2/", "DOB"), "DOB");
  assert.equal(viewFromUrl("https://juha.no/iono2/?view=unknown", "unknown"), "TGO");
  assert.equal(viewFromUrl("https://juha.no/iono2/?view=DMB", "DMB"), "TGO");
});
test("view navigation preserves the rest of the URL and supports history states", () => {
  const original="https://juha.no/iono2/?product=rti#plots";
  const first=urlForView(original,"TGO"), second=urlForView(first,"propagation");
  assert.equal(new URL(second).searchParams.get("product"),"rti");
  assert.equal(new URL(second).hash,"#plots");
  assert.equal(viewFromUrl(first),"TGO");
  assert.equal(viewFromUrl(second),"propagation");
  assert.equal(urlForView(second,"propagation"),second);
});
test("Norway crop has a shareable URL and World restores the full map", () => {
  const original = "https://juha.no/iono2/?view=propagation&product=rti#map";
  const norway = urlForMapRegion(original,"norway");
  assert.equal(mapRegion(norway),"norway");
  assert.equal(new URL(norway).searchParams.get("view"),"propagation");
  assert.equal(urlForMapRegion(norway,"world"),original);
  assert.equal(mapRegion(original),"world");
});
test("plot-order cookie round trips across the dashboard URL migration", () => {
  const order=["TGO:rti:SGO","TGO:lfm:Ramfjordmoen:ionogram","DOB:rti:SGO"];
  const cookie=plotOrderCookie(order);
  assert.deepEqual(readPlotOrderCookie("unrelated=value; "+cookie.split(";")[0]),order);
  assert.match(cookie,/Path=\/;/);
  assert.match(cookie,/Max-Age=31536000/);
  assert.match(cookie,/SameSite=Lax; Secure$/);
  assert.ok(!plotOrderCookie(order,false).endsWith("Secure"));
  assert.equal(readPlotOrderCookie("other=value"),null);
  assert.equal(readPlotOrderCookie("iono2_plot_order=%broken"),null);
  assert.deepEqual(readPlotOrderCookie("iono2_plot_order=%5B%5D"),[]);
});
test("reordering one station retains other stations and currently hidden plots", () => {
  const old=["TGO:rti:SGO","TGO:rti:Juliusruh","DOB:rti:SGO","TGO:lfm:SGO:ionogram"];
  const merged=mergePlotOrder(old,["TGO:rti:Juliusruh","TGO:rti:SGO"]);
  assert.deepEqual(merged.filter(k=>k.includes(":rti:")&&k.startsWith("TGO:")),["TGO:rti:Juliusruh","TGO:rti:SGO"]);
  assert.ok(merged.includes("DOB:rti:SGO"));
  assert.ok(merged.includes("TGO:lfm:SGO:ionogram"));
  assert.deepEqual(readPlotOrderCookie(plotOrderCookie(merged).split(";")[0]),merged);
});
test("a saved group order supersedes old individual-subplot positions", () => {
  const products=[{station:"TGO",kind:"spectrum",transmitter:"",quantity:"spectrum"},
    {station:"TGO",kind:"rti",transmitter:"SGO",quantity:"snr"}];
  const desired=["TGO:spectrum::spectrum","TGO:rti:SGO"];
  assert.deepEqual(groupProducts(products,["TGO:rti:SGO:snr",...desired]).map(g=>g.key),desired);
  const merged=mergePlotOrder(["TGO:rti:SGO:snr","DOB:rti:SGO:snr"],desired);
  assert.ok(!merged.includes("TGO:rti:SGO:snr"));
  assert.ok(merged.includes("DOB:rti:SGO:snr"));
});
