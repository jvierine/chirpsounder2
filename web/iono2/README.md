# iono2 browser UI

This directory contains the static browser interface deployed at
`https://juha.no/iono2/`.

The application deliberately keeps scientific plotting in Rust/WASM. It loads
the deployed `rustplotlib_web` package for heatmaps, line plots, ionograms, and
the all-station propagation map. JavaScript is limited to DOM state, catalogue
requests, tab ordering, and passing receiver-prepared arrays to rustplotlib.

The production `iono-browser-server` is a lightweight Rust service. It indexes
the shovel archive in the background and serves receiver-prepared products from
its local store, so normal browser requests do not scan or read the slow shovel
filesystem. The service and its WASM package are deployed separately from these
three static files.

## Deployment

Deploy `index.html`, `style.css`, and `app.js` to `/home/j/iono2-web/` on
`j@juha.no`. Preserve the existing `pkg/` and asset directories, which contain
the rustplotlib WASM build and logos. Back up the current static files before an
update and use atomic renames for the final replacement.

The station-tab order is stored in the browser under `iono2-tab-order`; the
active receiver or propagation view is stored under `iono2-active-view`. Plot
order is edited from the compact arranger in the settings dialog and stored
under `iono2-order`.

The default plot layout automatically fits as many compact columns as the
browser width permits, while viewports at 850 px or narrower always use one
column. The propagation view offers persistent Norway and world map extents;
both maps are rendered by rustplotlib WASM.
