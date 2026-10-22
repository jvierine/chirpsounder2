const {test} = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const path = require('node:path');

const php = fs.readFileSync(path.join(__dirname, '../web/index.php'), 'utf8');
const script = [...php.matchAll(/<script>([\s\S]*?)<\/script>/g)].at(-1)[1]
    .replace(/<\?php[\s\S]*?\?>/g, '60');

function image(name) {
    let src = `/iono/${name}?v=1-100`;
    return {
        dataset: {refreshSrc: `/iono/${name}`, version: '1-100'}, writes: 0,
        get src() { return src; },
        set src(value) { src = value; this.writes++; },
    };
}

function dashboard() {
    const state = {active: null, calls: 0, manifest: {}, time: 1000000};
    const overlay = {classList: {contains: () => false}, addEventListener() {}};
    const document = {
        hidden: false,
        getElementById: id => id === 'overlay' ? overlay : {},
        querySelectorAll: () => [],
        querySelector: () => state.active && {querySelectorAll: () => state.active},
        addEventListener() {},
    };
    const context = vm.createContext({
        document, localStorage: {getItem: () => null},
        window: {setInterval() {}}, queueMicrotask, AbortSignal,
        Date: {now: () => state.time},
        fetch: async () => {
            state.calls++;
            if (state.fail) throw Error('offline');
            return {ok: true, json: async () => state.manifest};
        },
    });
    vm.runInContext(script, context);
    return {state, document, refresh: context.refreshDashboardImages};
}

test('unchanged plots cause no image downloads across repeated polls', async () => {
    const {state, refresh} = dashboard();
    const img = image('a.png');
    state.active = [img]; state.manifest = {'a.png': '1-100'};
    await refresh(); state.time += 61000; await refresh();
    assert.equal(state.calls, 2);
    assert.equal(img.writes, 0);
});

test('only selected receiver updates; switching reuses the cached manifest', async () => {
    const {state, refresh} = dashboard();
    const a = image('a.png'), b = image('b.png');
    state.active = [a]; state.manifest = {'a.png': '2-200', 'b.png': '3-300'};
    await refresh();
    assert.equal(a.src, '/iono/a.png?v=2-200');
    assert.equal(b.writes, 0);
    state.active = [b]; await refresh();
    assert.equal(b.src, '/iono/b.png?v=3-300');
    assert.equal(state.calls, 1);
    await refresh(); assert.equal(b.writes, 1);
});

test('background pages do not poll or download images', async () => {
    const {state, document, refresh} = dashboard();
    document.hidden = true; await refresh();
    assert.equal(state.calls, 0);
});

test('failed polling preserves images and allows a subsequent retry', async () => {
    const {state, refresh} = dashboard();
    const img = image('a.png'); state.active = [img]; state.fail = true;
    await refresh(); assert.equal(img.writes, 0);
    state.fail = false; state.manifest = {'a.png': '2-200'};
    await refresh(); assert.equal(img.writes, 1);
});
