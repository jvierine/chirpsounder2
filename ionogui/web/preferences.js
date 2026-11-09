export const receiverViews = ["TGO", "DOB", "KHO", "W2NAF", "KREA"];
const views = [...receiverViews, "propagation"];
const canonicalView = value => views.find(v => v.toLowerCase() === value?.toLowerCase());
export function viewFromUrl(href, fallback = "TGO") {
  return canonicalView(new URL(href).searchParams.get("view")) || canonicalView(fallback) || "TGO";
}
export function urlForView(href, view) {
  const url = new URL(href);
  url.searchParams.set("view", canonicalView(view) || "TGO");
  return url.href;
}
export function mapRegion(href) {
  return new URL(href).searchParams.get("region") === "norway" ? "norway" : "world";
}
export function urlForMapRegion(href, region) {
  const url = new URL(href);
  if (region === "norway") url.searchParams.set("region", region);
  else url.searchParams.delete("region");
  return url.href;
}
const cookieName = "iono2_plot_order";
export const cleanPlotOrder = value => Array.isArray(value)
  ? [...new Set(value.filter(k => typeof k === "string" && receiverViews.some(rx => k.startsWith(rx + ":"))))]
  : [];
export function readPlotOrderCookie(cookies) {
  const entry = cookies.split(";").map(v => v.trim()).find(v => v.startsWith(cookieName + "="));
  if (!entry) return null;
  try {
    const value = JSON.parse(decodeURIComponent(entry.slice(cookieName.length + 1)));
    return Array.isArray(value) ? cleanPlotOrder(value) : null;
  } catch { return null; }
}
export function plotOrderCookie(order, secure = true) {
  return `${cookieName}=${encodeURIComponent(JSON.stringify(cleanPlotOrder(order)))}; Path=/; Max-Age=31536000; SameSite=Lax${secure ? "; Secure" : ""}`;
}
export function mergePlotOrder(previous, current) {
  const next = cleanPlotOrder(current), moved = new Set(next);
  return [...cleanPlotOrder(previous).filter(k => !moved.has(k) && !next.some(group => k.startsWith(group + ":"))), ...next];
}
