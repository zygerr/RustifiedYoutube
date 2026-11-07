(() => {
  if (location.origin !== 'https://music.youtube.com' || window.__rustifiedMedia) return;
  const session = { generation: 0, target: '', playing: false, volume: .5, video: false, ready: false, counterpartUntil: 0, changed: 0, error: null, modeApplied: false, appliedMode: null, transition: null, diagnostic: false, lastPosition: 0, lastDuration: 0, seekRequest: null, seekAck: 0 };
  let last = '', boundVideo = null, ended = false, playAttempt = 0, lastAd = false;
  if (Number.isFinite(window.__rustifiedInitialVolume)) session.volume=Math.max(0,Math.min(1,window.__rustifiedInitialVolume));
  const send = data => window.ipc.postMessage(JSON.stringify(data));
  const api = () => document.querySelector('ytmusic-player')?.playerApi || document.querySelector('ytmusic-player-bar')?.playerApi || document.getElementById('movie_player');
  const video = () => api()?.getVideoElement?.() || api()?.querySelector?.('video') || document.querySelector('#movie_player video') || document.querySelector('ytmusic-player video') || document.querySelector('video');
  const position = () => Number(api()?.getCurrentTime?.() ?? video()?.currentTime ?? 0);
  const duration = () => Number(api()?.getDuration?.() ?? video()?.duration ?? 0);
  const capVolume = media => {
    if (!media) return;
    if (media.volume > session.volume) media.volume=session.volume;
    if (session.volume === 0 && !media.muted) media.muted=true;
  };
  // YouTube replaces media elements and restores its saved volume on source
  // changes. Enforce the native ceiling synchronously, before a sample plays,
  // while preserving lower levels applied by YouTube's loudness normalization.
  const mediaPrototype=globalThis.HTMLMediaElement?.prototype;
  const volumeDescriptor=mediaPrototype && Object.getOwnPropertyDescriptor(mediaPrototype,'volume');
  if (volumeDescriptor?.configurable && volumeDescriptor.set) {
    Object.defineProperty(mediaPrototype,'volume',{
      ...volumeDescriptor,
      set(value) {
        const number=Number(value);
        volumeDescriptor.set.call(this,Number.isFinite(number) ? Math.min(number,session.volume) : value);
      }
    });
  }
  if (mediaPrototype?.play) {
    const play=mediaPrototype.play;
    mediaPrototype.play=function(...args) { capVolume(this); return play.apply(this,args); };
  }
  const applyVolume = () => {
    capVolume(video());
  };
  for (const event of ['volumechange','loadstart','loadedmetadata','canplay','play','playing']) {
    document.addEventListener(event,event => { capVolume(event.target); applyVolume(); },true);
  }
  if (typeof MutationObserver !== 'undefined') new MutationObserver(() => {
    document.querySelectorAll('audio,video').forEach(capVolume);
  }).observe(document,{subtree:true,childList:true});
  const trace = (phase,target=null) => { if (session.diagnostic) send({type:'trace',phase,position:video()?.currentTime || 0,target,video:session.video,ready:video()?.readyState || 0,seeking:!!video()?.seeking,api_state:api()?.getPlayerState?.() ?? -1}); };
  const seek = (seconds,revision=0) => {
    session.seekRequest={target:seconds,revision,requested:performance.now()};
    // Reset rollover detection until an actual, settled clock is observed.
    session.lastPosition=0;
    const player=api();
    const media=video();
    if (typeof player?.seekTo === 'function') player.seekTo(seconds,true);
    else if (media) media.currentTime=seconds;
    trace('seek_applied',seconds);
  };
  const isAd = () => !!document.getElementById('movie_player')?.classList.contains('ad-showing');
  const dataId = () => {
    const player = api();
    return player?.getVideoData?.()?.video_id || player?.getPlayerResponse?.()?.videoDetails?.videoId || '';
  };
  const setMode = () => {
    if (session.transition) return;
    session.appliedMode = session.video;
    const button = document.querySelector(session.video ? 'ytmusic-av-toggle .video-button' : 'ytmusic-av-toggle .song-button');
    if (button && !button.disabled && button.getAttribute('aria-pressed') !== 'true' && !button.hasAttribute('selected')) {
      session.transition = { source: video()?.currentSrc, started: performance.now(), loaded: false, seek: null, seekRevision: 0 };
      session.counterpartUntil = performance.now() + 10000;
      session.changed = performance.now();
      applyVolume();
      button.click();
      applyVolume();
      trace('mode_started');
    }
  };
  const style = () => {
    if (!document.head || document.getElementById('rustified-video-surface')) return;
    const sheet = document.createElement('style');
    sheet.id = 'rustified-video-surface';
    sheet.textContent = `
      html, body { margin:0!important; overflow:hidden!important; background:#000!important; }
      body * { visibility:hidden!important; }
      ytmusic-player-page, ytmusic-player, #player, #movie_player, .html5-video-container {
        position:fixed!important; inset:0!important; width:100%!important; height:100%!important;
        max-width:none!important; max-height:none!important; margin:0!important; padding:0!important;
        transform:none!important; background:#000!important;
      }
      #movie_player, #movie_player * { visibility:visible!important; }
      video { position:absolute!important; inset:0!important; width:100%!important; height:100%!important; object-fit:contain!important; }
      .ytp-chrome-top, .ytp-chrome-bottom, .ytp-gradient-top, .ytp-gradient-bottom, .ytp-title, .ytp-large-play-button { display:none!important; }
      #movie_player { pointer-events:none!important; }
    `;
    document.head.appendChild(sheet);
  };
  window.__rustifiedMedia = {
    command(command) {
      const player = api();
      const media = video();
      switch (command.op) {
        case 'diagnostic': session.diagnostic=true; break;
        case 'play':
          if (!/^[\w-]{11}$/.test(command.id)) return;
          session.generation = command.generation;
          session.target = command.id;
          session.playing = !command.paused;
          session.changed = performance.now();
          session.counterpartUntil = 0;
          session.error = null;
          session.modeApplied = false;
          session.appliedMode = null;
          session.transition = null;
          session.lastPosition=0;
          session.lastDuration=0;
          session.seekRequest=null;
          session.seekAck=0;
          ended = false;
          applyVolume();
          document.dispatchEvent(new CustomEvent('yt-navigate', { detail: { endpoint: { watchEndpoint: { videoId: command.id, playlistId: 'RDAMVM' + command.id } } } }));
          break;
        case 'toggle':
          session.playing = media ? media.paused : !session.playing;
          if (session.playing && session.target && dataId() !== session.target) {
            window.__rustifiedMedia.command({op:'play',id:session.target,generation:session.generation});
            return;
          }
          if (session.playing) { applyVolume(); player?.playVideo?.(); media?.play().catch(() => {}); }
          else { player?.pauseVideo?.(); media?.pause(); }
          break;
        case 'pause': session.playing = false; player?.pauseVideo?.(); media?.pause(); break;
        case 'volume':
          session.volume = Math.min(1, Math.max(0, command.value));
          player?.setVolume?.(session.volume * 100);
          if (media) { media.volume = session.volume; media.muted = session.volume === 0; }
          if (session.volume > 0) player?.unMute?.();
          applyVolume();
          break;
        case 'seek':
          if (Number.isFinite(command.value)) {
            if (session.transition) { session.transition.seek = Math.max(0, command.value); session.transition.seekRevision=command.revision || 0; trace('seek_queued',command.value); }
            else {
              seek(Math.max(0,command.value),command.revision || 0);
            }
          }
          break;
        case 'mode': session.video = !!command.video; if (session.generation) setMode(); break;
      }
    }
  };
  document.addEventListener('keydown', event => {
    if (!event.isTrusted) return;
    if (event.ctrlKey && !event.altKey && event.code === 'KeyQ') {
      event.preventDefault(); send({type:'shortcut',action:'quit'});
    } else if (event.ctrlKey && event.altKey && event.code === 'Space') {
      event.preventDefault(); send({type:'shortcut',action:'toggle'});
    }
  }, true);
  setInterval(() => {
    style();
    const media = video();
    const player = api();
    applyVolume();
    if (!session.ready && document.querySelector('ytmusic-player-bar') && document.readyState === 'complete') {
      session.ready = true;
      send({ type: 'ready' });
    }
    if (media && media !== boundVideo) {
      boundVideo = media;
      media.addEventListener('loadstart', () => { if (session.transition) session.transition.loaded = true; });
      media.addEventListener('ended', () => {
        if (!session.generation || isAd() || lastAd || media !== video() || !media.ended || performance.now() - session.changed < 500) return;
        ended = true;
        session.playing = false;
        media.pause();
      }, true);
    }
    if (!session.generation) { media?.pause(); return; }
    let id = dataId();
    const ad = isAd();
    lastAd = ad;
    const nearEnd = session.lastDuration > 0 && session.lastPosition >= session.lastDuration - 1.5;
    const seekReachedEnd = session.seekRequest && nearEnd && Math.abs(session.lastPosition-session.seekRequest.target) < 1.5;
    // The official player can advance its own queue before HTML 'ended' reaches
    // listeners. Keep the app's queue authoritative using the last media clock.
    if (!ad && session.playing && !session.transition && (!session.seekRequest || seekReachedEnd) && nearEnd &&
        ((id && id !== session.target) || (media && !media.seeking && position() + 1 < session.lastPosition &&
          (!session.seekRequest || performance.now()-session.seekRequest.requested > 500)))) {
      ended=true;
      session.playing=false;
      media?.pause();
    }
    if (!ad && session.transition && id !== session.target && id && performance.now() < session.counterpartUntil) {
      session.target = id;
    }
    const matches = id === session.target;
    if (!matches && !ad) {
      if (media && !media.paused) media.pause();
      if (session.playing && performance.now() - session.changed > 30000) session.error = 'YouTube could not open this track. Reconnect your account or try another result.';
    } else if (media) {
      if (!ad && !session.modeApplied && media.readyState >= 1) { session.modeApplied = true; setMode(); }
      if (session.playing && media.paused && !ended && performance.now() - playAttempt > 1000) {
        playAttempt = performance.now();
        applyVolume();
        player?.playVideo?.();
        media.play().catch(() => {});
      }
      if (!session.playing && !media.paused) media.pause();
      if (media.error) session.error = 'The YouTube player could not play this track.';
      const transition = session.transition;
      if (!ad && transition && media.readyState >= 3) {
        const elapsed = performance.now() - transition.started;
        const state = player?.getPlayerState?.();
        if (elapsed > 1800 && (state === undefined || state === 1 || state === 2)) {
          session.transition = null;
          trace('mode_settled',transition.seek);
          if (session.appliedMode !== session.video) {
            setMode();
            if (session.transition) { session.transition.seek = transition.seek; session.transition.seekRevision=transition.seekRevision; }
            else if (transition.seek !== null) seek(transition.seek,transition.seekRevision);
          } else if (transition.seek !== null) {
            seek(transition.seek,transition.seekRevision);
          }
        }
      }
    }
    if (session.seekRequest && matches && !ad && !session.transition && media && !media.seeking && media.readyState >= 2 &&
        Math.abs(position()-session.seekRequest.target) < 1.5) {
      session.seekAck=session.seekRequest.revision;
      session.seekRequest=null;
    }
    if (session.seekRequest) trace('seek_waiting',position());
    const videoButton = document.querySelector('ytmusic-av-toggle .video-button');
    const state = {
      type: 'state', generation: session.generation, video_id: id,
      seek_ack: session.seekAck,
      title: String(player?.getVideoData?.()?.title || '').slice(0,512),
      transition: !!session.transition,
      position: matches && Number.isFinite(position()) ? Math.round(position() * 100) / 100 : 0,
      duration: matches && Number.isFinite(duration()) ? duration() : 0,
      playing: !!((matches || ad) && media && !media.paused && !media.ended),
      buffering: session.playing && !session.error && (!matches || !media || media.readyState < 3),
      ended, ad, video_width: media?.videoWidth || 0, video_height: media?.videoHeight || 0,
      video_available: !!(videoButton && !videoButton.disabled) || !!media?.videoWidth,
      error: session.error
    };
    const encoded = JSON.stringify(state);
    if (encoded !== last) { last = encoded; send(state); }
    ended = false;
    // A seek into the final frame can hand off to YouTube's next item before
    // 'seeked'. Remember the API's observed target clock so that handoff still
    // advances the native queue, without treating an in-progress seek as EOF.
    const observedSeek = session.seekRequest && Math.abs(position()-session.seekRequest.target) < 1.5;
    if (!ad && matches && media && ((!media.seeking && media.readyState >= 2) || observedSeek) && Number.isFinite(duration()) && duration() > 0) {
      session.lastPosition=position();
      session.lastDuration=duration();
    }
  }, 200);
})();
