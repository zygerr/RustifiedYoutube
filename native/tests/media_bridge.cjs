const {test} = require('node:test');
const assert = require('node:assert/strict');
const vm = require('node:vm');
const fs = require('node:fs');
const path = require('node:path');

function player(initialVolume = .5) {
  let now = 10000, interval, id = 'Fixture0001', clock = 10;
  const messages = [];
  const events = new Map(), playedVolumes = [];
  const emit = (event, target) => {for (const listener of events.get(event) || []) listener({target});};
  class MockMedia {
    constructor() {
      Object.assign(this,{readyState:4,seeking:false,paused:false,ended:false,duration:200,currentTime:10,
        _volume:1,muted:false,videoWidth:640,videoHeight:360});
    }
    get volume() {return this._volume;}
    set volume(value) {if(this._volume !== value) {this._volume=value; emit('volumechange',this);}}
    addEventListener() {}
    pause() {this.paused=true;}
    play() {playedVolumes.push(this.volume);emit('play',this);this.paused=false;return Promise.resolve();}
  }
  let media = new MockMedia(), apiVolume = 100;
  let acceptSeek = true;
  const api = {
    getVideoElement: () => media, getVideoData: () => ({video_id: id}),
    getCurrentTime: () => clock, getDuration: () => 200,
    getPlayerState: () => media.paused ? 2 : 1,
    getVolume: () => apiVolume, setVolume(value) {apiVolume=value;media.volume=value/100;},
    isMuted: () => media.muted, mute() {media.muted=true;}, unMute() {media.muted=false;},
    seekTo(target) {
      if (acceptSeek) {
        clock = media.currentTime = target;
        media.seeking = true; media.readyState = 1;
      }
    },
    playVideo() {media.paused = false;}, pauseVideo() {media.paused = true;}
  };
  const document = {
    readyState: 'complete', head: {},
    addEventListener(event,listener) {if (!events.has(event)) events.set(event,[]);events.get(event).push(listener);},
    querySelectorAll: () => [media],
    querySelector(selector) {
      if (selector === 'ytmusic-player') return {playerApi: api};
      if (selector === 'ytmusic-player-bar') return {};
      return null;
    },
    getElementById: () => ({classList: {contains: () => false}}),
    dispatchEvent(event) {id = event.detail.endpoint.watchEndpoint.videoId;}
  };
  const context = vm.createContext({
    document, location: {origin: 'https://music.youtube.com'},
    window: {__rustifiedInitialVolume:initialVolume,ipc: {postMessage: text => messages.push(JSON.parse(text))}},
    HTMLMediaElement:MockMedia,
    performance: {now: () => now},
    CustomEvent: class {constructor(type, options) {Object.assign(this, options);}},
    setInterval(callback) {interval = callback;}
  });
  vm.runInContext(fs.readFileSync(path.join(__dirname, '../src/media_bridge.js'), 'utf8'), context);
  const command = value => context.window.__rustifiedMedia.command(value);
  const tick = (elapsed = 200) => {
    now += elapsed; interval();
    return messages.filter(message => message.type === 'state').at(-1);
  };
  command({op: 'play', id, generation: 1}); tick();
  return {
    command, tick, get media() {return media;}, playedVolumes,
    replaceMedia() {media=new MockMedia();return media;},
    resetYouTubeVolume(value) {apiVolume=value*100;media.volume=value;},
    rejectSeek() {acceptSeek = false;},
    clock(value) {clock = media.currentTime = value;},
    handoff() {id = 'Fixture0002'; clock = media.currentTime = 0; media.seeking = false; media.readyState = 4;}
  };
}

test('a seek is acknowledged only after the selected clock settles', () => {
  const p = player();
  p.command({op: 'seek', value: 90, revision: 1});
  assert.equal(p.tick().seek_ack, 0);
  p.media.seeking = false; p.media.readyState = 4;
  assert.equal(p.tick().seek_ack, 1);
});

test('YouTube volume resets are capped synchronously without waiting for a poll', () => {
  const p=player(.2);
  p.resetYouTubeVolume(1);
  assert.equal(p.media.volume,.2);
  p.media.play();
  assert.ok(p.playedVolumes.every(volume => volume<=.2));
});

test('new media elements are limited before their first play call', () => {
  const p=player(.25), replacement=p.replaceMedia();
  assert.equal(replacement.volume,1);
  replacement.play();
  assert.deepEqual(p.playedVolumes,[.25]);
});

test('normalization below the selected level is preserved across polls', () => {
  const p=player(.6);
  p.media.volume=.35;
  p.tick();
  assert.equal(p.media.volume,.35);
});

test('mute survives YouTube unmute and volume restores, then user volume can increase', () => {
  const p=player(0);
  p.resetYouTubeVolume(1);p.media.muted=false;p.media.play();
  assert.equal(p.media.volume,0);
  assert.deepEqual(p.playedVolumes,[0]);
  p.command({op:'volume',value:.7});
  assert.equal(p.media.volume,.7);assert.equal(p.media.muted,false);
});

test('rapid seeks confirm the latest request, never the old clock', () => {
  const p = player();
  p.command({op: 'seek', value: 40, revision: 1});
  p.command({op: 'seek', value: 140, revision: 2});
  p.media.seeking = false; p.media.readyState = 4; p.clock(40);
  assert.equal(p.tick().seek_ack, 0);
  p.clock(140);
  assert.equal(p.tick().seek_ack, 2);
});

test('an unaccepted seek near the end cannot skip the current track', () => {
  const p = player(); p.rejectSeek();
  p.command({op: 'seek', value: 199, revision: 1});
  const state = p.tick(1000);
  assert.equal(state.ended, false);
  assert.equal(state.position, 10);
  assert.equal(state.seek_ack, 0);
});

test('a final-frame seek hands control back to the native queue before seeked', () => {
  const p = player();
  p.command({op: 'seek', value: 199, revision: 1});
  assert.equal(p.tick().ended, false);
  p.handoff();
  const state = p.tick(600);
  assert.equal(state.ended, true);
  assert.equal(state.playing, false);
  assert.equal(p.media.paused, true);
});
