import { readFileSync } from "node:fs";
import vm from "node:vm";
import test from "node:test";
import assert from "node:assert/strict";

const source = readFileSync(new URL("../ui/playback.js", import.meta.url), "utf8");

function fixture(origin = "https://music.youtube.com", hasVideo = true) {
  const clicks = [];
  const video = {
    paused: true,
    muted: false,
    play() { this.paused = false; return Promise.resolve(); },
    pause() { this.paused = true; },
  };
  const context = vm.createContext({
    location: { origin },
    document: {
      querySelector(selector) {
        if (selector === "video") return hasVideo ? video : null;
        if (selector === "ytmusic-player-bar") {
          return { querySelector: (selector) => ({ click: () => clicks.push(selector) }) };
        }
        return null;
      },
    },
  });
  vm.runInContext(source, context);
  return { video, clicks, run: (action) => context.controlPlayback(action) };
}

test("play/pause toggles the actual video element", () => {
  const app = fixture();
  app.run("play_pause");
  assert.equal(app.video.paused, false);
  app.run("play_pause");
  assert.equal(app.video.paused, true);
});

test("track changes use YouTube Music's own queue buttons", () => {
  const app = fixture();
  app.run("next");
  app.run("previous");
  assert.deepEqual(app.clicks, [".next-button", ".previous-button"]);
});

test("mute preserves playback state and toggles back", () => {
  const app = fixture();
  app.run("mute");
  assert.equal(app.video.muted, true);
  assert.equal(app.video.paused, true);
  app.run("mute");
  assert.equal(app.video.muted, false);
});

test("native controls do nothing on a sign-in or unrelated page", () => {
  for (const origin of ["https://accounts.google.com", "https://music.youtube.com.evil.test"]) {
    const app = fixture(origin);
    for (const action of ["play_pause", "next", "previous", "mute"]) app.run(action);
    assert.equal(app.video.paused, true);
    assert.equal(app.video.muted, false);
    assert.deepEqual(app.clicks, []);
  }
});

test("a page without a player can receive controls safely", () => {
  const context = vm.createContext({
    location: { origin: "https://music.youtube.com" },
    document: { querySelector: () => null },
  });
  vm.runInContext(source, context);
  for (const action of ["play_pause", "next", "previous", "mute"]) {
    assert.doesNotThrow(() => context.controlPlayback(action));
  }
});

test("a rejected play request does not create an unhandled rejection", async () => {
  const context = vm.createContext({
    location: { origin: "https://music.youtube.com" },
    document: {
      querySelector: (selector) => selector === "video"
        ? { paused: true, play: () => Promise.reject(new Error("autoplay denied")) }
        : null,
    },
  });
  vm.runInContext(source, context);
  context.controlPlayback("play_pause");
  await new Promise(setImmediate);
});
