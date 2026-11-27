import { expect, test } from "bun:test";
import { readFile } from "node:fs/promises";

// What a click in the Settings window writes to settings.json, without a window or a core.
const source = await readFile(new URL("../defaults/frame/settings/main.js", import.meta.url), "utf8");
// settings.json as the mod sees it through readTextFile / writeTextFile.
const disk: { raw: string | null; writes: number; fail?: string; readFail?: string; toasts: any[] } = { raw: null, writes: 0, toasts: [] };
const values: Record<string, any> = {
  View: class {},
  appHome: () => "/home",
  notify: (n: any) => { disk.toasts.push(n); },
  readTextFile: async () => {
    if (disk.readFail) throw new Error(disk.readFail);
    return disk.raw;
  },
  writeTextFile: async (_path: string, text: string, expected: string | null) => {
    disk.writes++;
    if (disk.fail) throw new Error(disk.fail);
    if (expected !== disk.raw) throw new Error("changed since read");
    disk.raw = text;
  },
};
const element: any = new Proxy({}, { get: () => (..._args: unknown[]) => element });
let auto = false;
values.v_flex = values.h_flex = values.div = () => element;
values.updates = () => ({ auto, checks: false, app: { updates: false, version: "1" }, core: {} });
const names = [...source.matchAll(/^import \{([^}]*)\}/gm)].flatMap(m => m[1].split(",").map(n => n.trim()));
const args = names.map(name => values[name] ?? (() => undefined));
const Settings = new Function(...names, source.replace(/^import .*;\n/gm, "").replace("export default class Settings", "return class Settings"))(...args);

/** Runs `edit` through the Settings window's change() on `raw`; the toasts it posts. */
async function change(raw: string | null, edit: (q: Record<string, any>) => void, fail?: string, readFail?: string) {
  Object.assign(disk, { raw, writes: 0, fail, readFail, toasts: [] });
  const view = new Settings();
  let done: () => void;
  const finished = new Promise<void>((resolve) => (done = resolve));
  view.change(edit, { spawn: (f: (cx: any) => Promise<void>) => f({ notify: () => done() }) });
  await finished;
  return { view, toasts: disk.toasts };
}

test("a change writes settings.json, keeping what else it holds", async () => {
  const { view, toasts } = await change('{"font_size": 15}', (q) => (q.dock_bounce = true));
  expect(JSON.parse(disk.raw!)).toEqual({ font_size: 15, dock_bounce: true });
  expect(view.person).toEqual({ font_size: 15, dock_bounce: true });
  expect(toasts).toEqual([]);
});

test("a broken settings.json is never written over, and says so", async () => {
  const broken = '{"font_size": 15, "theme": "x",}';
  const { toasts } = await change(broken, (q) => (q.dock_bounce = true));
  expect(disk.raw).toBe(broken);
  expect(disk.writes).toBe(0);
  expect(toasts).toEqual([{ title: "settings.json has errors: fix it first", level: "error" }]);
});

test("settings.json that isn't an object counts as broken", async () => {
  for (const raw of ["null", "[]", "3"]) {
    const { toasts } = await change(raw, (q) => (q.dock_bounce = true));
    expect(disk.raw).toBe(raw);
    expect(disk.writes).toBe(0);
    expect(toasts).toEqual([{ title: "settings.json has errors: fix it first", level: "error" }]);
  }
});

test("a write that keeps failing is reported, not dropped", async () => {
  const { toasts } = await change("{}", (q) => (q.dock_bounce = true), "read-only file system");
  expect(disk.writes).toBe(3);
  expect(disk.raw).toBe("{}");
  expect(toasts).toEqual([{ title: "Not saved to settings.json", body: "read-only file system", level: "error" }]);
});

test("a settings.json that can't be read, or an edit that throws, says why", async () => {
  const unread = await change("{}", (q) => (q.dock_bounce = true), undefined, "permission denied");
  expect(disk.writes).toBe(0);
  expect(unread.toasts).toEqual([{ title: "Cannot read settings.json", body: "permission denied", level: "error" }]);
  const thrown = await change("{}", () => { throw new Error("no such section"); });
  expect(disk.writes).toBe(0);
  expect(thrown.toasts).toEqual([{ title: "Cannot read settings.json", body: "no such section", level: "error" }]);
});

test("automatic updates respond before saving and retain an explicit choice", async () => {
  Object.assign(disk, { raw: "{}", writes: 0, fail: undefined, readFail: undefined, toasts: [] });
  auto = false;
  const view = new Settings();
  view.heading = view.row = view.button = () => element;
  let shown = false;
  let click: (_event: unknown, cx: unknown) => void = () => {};
  view.toggle = (_theme: unknown, _id: unknown, on: boolean, handler: typeof click) => { shown = on; click = handler; return element; };
  let finished: Promise<void> = Promise.resolve();
  const cx = { notify: () => {}, spawn: (f: (cx: unknown) => Promise<void>) => { finished = f(cx); } };
  view.updates({});
  expect(shown).toBe(false);
  click(null, cx);
  view.updates({});
  expect(shown).toBe(true);
  await finished;
  expect(JSON.parse(disk.raw!)).toEqual({ auto_update: true });
  auto = true;
  view.updates({});
  click(null, cx);
  await finished;
  expect(JSON.parse(disk.raw!)).toEqual({ auto_update: false });
});

test("automatic update switch reverts when saving fails", async () => {
  Object.assign(disk, { raw: "{}", writes: 0, fail: "read-only", readFail: undefined, toasts: [] });
  auto = false;
  const view = new Settings();
  view.heading = view.row = view.button = () => element;
  let shown = false;
  let click: (_event: unknown, cx: unknown) => void = () => {};
  view.toggle = (_theme: unknown, _id: unknown, on: boolean, handler: typeof click) => { shown = on; click = handler; return element; };
  let finished: Promise<void> = Promise.resolve();
  const cx = { notify: () => {}, spawn: (f: (cx: unknown) => Promise<void>) => { finished = f(cx); } };
  view.updates({});
  click(null, cx);
  view.updates({});
  expect(shown).toBe(true);
  await finished;
  view.updates({});
  expect(shown).toBe(false);
  expect(disk.toasts[0].title).toBe("Not saved to settings.json");
});
