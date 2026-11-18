(() => {
  if (location.origin !== 'https://music.youtube.com' || window.__rustifiedMedia) return;
  const session = { generation: 0, target: '', playing: false, volume: .5, video: false, ready: false, counterpartUntil: 0, changed: 0, error: null, modeApplied: false, appliedMode: null, transition: null, diagnostic: false, lastPosition: 0, lastDuration: 0, seekRequest: null, seekAck: 0, restore: null, restoreSeek: false, fallbackMode: null, stalledSent: false, recoveryProbe: false, probeIgnored: false };
  let last = '', boundVideo = null, ended = false, playAttempt = 0, lastAd = false;
  if (Number.isFinite(window.__rustifiedInitialVolume)) session.volume=Math.max(0,Math.min(1,window.__rustifiedInitialVolume));
  const documentId = String(Date.now()) + '-' + Math.random().toString(36).slice(2);
  const send = data => window.ipc.postMessage(JSON.stringify({...data, document_id:documentId}));
  let lastIdentity='', confirmedIdentity={signed_in:null,auth_user:null}, signedOutSince=null;
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
  document.querySelectorAll('audio,video').forEach(capVolume);
  const capAddedMedia = node => {
    if (typeof HTMLMediaElement !== 'undefined' && node instanceof HTMLMediaElement) capVolume(node);
    node.querySelectorAll?.('audio,video').forEach(capVolume);
  };
  if (typeof MutationObserver !== 'undefined') new MutationObserver(records => {
    for (const record of records) record.addedNodes.forEach(capAddedMedia);
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
  const sameSong = (before,after) => {
    const words=text=>new Set(text.normalize('NFKC').toLowerCase().replace(/\([^)]*\)|\[[^\]]*\]/g,' ').replace(/[^\p{L}\p{N}]+/gu,' ').split(' ').filter(word=>word.length>2 && !/^(official|video|audio|lyrics|lyric|hd|4k|20\d\d)$/.test(word)));
    const a=words(before),b=words(after);
    return !a.size || !b.size || [...a].filter(word=>b.has(word)).length/Math.min(a.size,b.size)>=.6;
  };
  const setMode = () => {
    if (session.transition || (session.video===session.fallbackMode)) return;
    session.appliedMode = session.video;
    const button = document.querySelector(session.video ? 'ytmusic-av-toggle .video-button' : 'ytmusic-av-toggle .song-button');
    if (button && !button.disabled && button.getAttribute('aria-pressed') !== 'true' && !button.hasAttribute('selected')) {
      session.transition = { id:session.target,title:String(api()?.getVideoData?.()?.title || ''),position:position(),source: video()?.currentSrc, started: performance.now(), loaded: false, seek: null, seekRevision: 0 };
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
        case 'recovery_probe': session.recoveryProbe=true; break;
        case 'play':
          if (!/^[\w-]{11}$/.test(command.id)) return;
          session.generation = command.generation;
          session.target = command.id;
          session.playing = !command.paused;
          session.changed = performance.now();
          session.counterpartUntil = 0;
          session.error = null;
          session.stalledSent=false;
          session.modeApplied = false;
          session.appliedMode = null;
          session.transition = null;
          session.lastPosition=0;
          session.lastDuration=0;
          session.seekRequest=null;
          session.seekAck=0;
          session.fallbackMode=typeof command.fallback_mode==='boolean'?command.fallback_mode:null;
          session.restoreSeek=false;
          session.restore=Number.isFinite(command.position) && command.position>0 ? {position:command.position,revision:command.revision || 0} : null;
          ended = false;
          applyVolume();
          if (!command.direct || dataId()!==command.id) {
            if(session.recoveryProbe) session.probeIgnored=true;
            if(!session.recoveryProbe) document.dispatchEvent(new CustomEvent('yt-navigate', { detail: { endpoint: { watchEndpoint: { videoId: command.id, playlistId: 'RDAMVM' + command.id } } } }));
            session.recoveryProbe=false;
          }
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
  // Keyboard media keys, headset buttons and the Windows media flyout reach the
  // page through the Media Session API. Left to YouTube they pause the element
  // directly, the app still wants playback and resumes it a moment later, and
  // next/previous would advance YouTube's queue instead of ours. Claim those
  // actions before YouTube's scripts run and route them through the app.
  const mediaSession = globalThis.navigator?.mediaSession;
  if (mediaSession && typeof mediaSession.setActionHandler === 'function') {
    const setHandler = mediaSession.setActionHandler.bind(mediaSession);
    const want = playing => () => {
      if (session.generation && session.playing !== playing) send({type:'shortcut',action:'toggle'});
    };
    const routed = {
      play: want(true), pause: want(false), stop: want(false),
      nexttrack: () => send({type:'shortcut',action:'next'}),
      previoustrack: () => send({type:'shortcut',action:'previous'})
    };
    for (const [action, handler] of Object.entries(routed)) {
      try { setHandler(action, handler); } catch {}
    }
    mediaSession.setActionHandler = (action, handler) => {
      if (Object.hasOwn(routed, action)) return;
      return setHandler(action, handler);
    };
  }
  setInterval(() => {
    style();
    const media = video();
    const player = api();
    applyVolume();
    // A slow image/advertising resource mustn't hold authentication and player
    // initialization hostage until the entire page's load event completes.
    if (!session.ready && document.querySelector('ytmusic-player-bar') && document.readyState !== 'loading') {
      session.ready = true;
      send({ type: 'ready' });
    }
    if (session.ready) {
      const signed=window.ytcfg?.get?.('LOGGED_IN');
      const rawIndex=window.ytcfg?.get?.('SESSION_INDEX');
      const index=rawIndex===undefined || rawIndex===null ? NaN : Number(rawIndex);
      const selectedIndex=Number.isInteger(index) && index>=0 && index<=255?index:null;
      if (signed===true) {
        signedOutSince=null;
        confirmedIdentity={signed_in:true,auth_user:selectedIndex ?? (confirmedIdentity.signed_in===true?confirmedIdentity.auth_user:null)};
      } else if (signed===false) {
        // Bootstrap and account refresh can expose a transient false value.
        // Only an explicit, continuous signed-out state confirms logout.
        signedOutSince ??= performance.now();
        if (performance.now()-signedOutSince>=2000) confirmedIdentity={signed_in:false,auth_user:null};
      } else {
        signedOutSince=null;
      }
      const identity=confirmedIdentity;
      const encoded=JSON.stringify(identity);
      if(encoded!==lastIdentity) {lastIdentity=encoded;send({type:'session',...identity});}
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
    let id = session.probeIgnored ? '' : dataId();
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
    if (!ad && session.transition && id !== session.transition.id && id && performance.now() < session.counterpartUntil) {
      const transition=session.transition;
      const title=String(player?.getVideoData?.()?.title || '');
      if (media?.readyState>=2 && !sameSong(transition.title,title)) {
        // A stale official A/V toggle can point at the previous radio item.
        // Keep the requested song's original source for audio in that case.
        window.__rustifiedMedia.command({op:'play',id:transition.id,generation:session.generation,
          position:transition.seek ?? transition.position,revision:transition.seekRevision,
          paused:!session.playing,fallback_mode:session.video});
        return;
      }
      if (media?.readyState>=2) session.target = id;
    }
    const matches = id === session.target;
    const response=player?.getPlayerResponse?.();
    const status=response?.playabilityStatus?.status || '';
    const responseId=response?.videoDetails?.videoId;
    const elapsed=performance.now()-session.changed;
    const loginRequired=matches && (!responseId || responseId===session.target) && status==='LOGIN_REQUIRED' && elapsed>1000;
    const cannotOpen=(!matches && elapsed>4500) || ((!media || media.readyState<2) && elapsed>12000);
    if (!ad && !session.transition && !session.stalledSent &&
        (loginRequired || (cannotOpen && (session.playing || session.restore)))) {
      session.stalledSent=true;
      session.error=loginRequired ? 'Google requests sign-in for this track.' : 'YouTube is retrying the requested track…';
      send({type:'stalled',generation:session.generation,video_id:session.target,status:loginRequired?'LOGIN_REQUIRED':matches && ['ERROR','UNPLAYABLE'].includes(status)?status:'NAVIGATION_STALLED'});
    }
    if (!matches && !ad) {
      if (media && !media.paused) media.pause();

    } else if (media) {
      if (!ad && !session.modeApplied && media.readyState >= 1) { session.modeApplied = true; setMode(); }
      if (session.playing && !session.restore && !session.restoreSeek && media.paused && !ended && performance.now() - playAttempt > 1000) {
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
        if (elapsed > 1800 && (state === undefined || state === 1 || state === 2 || (!session.playing && media.paused && !media.seeking))) {
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
    if (session.restore && matches && !ad && session.modeApplied && !session.transition && media?.readyState>=2) {
      const restore=session.restore;session.restore=null;session.restoreSeek=true;seek(restore.position,restore.revision);
    }
    if (session.seekRequest && matches && !ad && !session.transition && media && !media.seeking && media.readyState >= 2 &&
        Math.abs(position()-session.seekRequest.target) < 1.5) {
      session.restoreSeek=false;
      session.seekAck=session.seekRequest.revision;
      session.seekRequest=null;
    }
    if (session.seekRequest) trace('seek_waiting',position());
    const videoButton = document.querySelector('ytmusic-av-toggle .video-button');
    const state = {
      type: 'state', generation: session.generation, video_id: id,
      seek_ack: session.seekAck,
      title: String(player?.getVideoData?.()?.title || '').slice(0,512),
      transition: !!session.transition || !!session.restore || session.restoreSeek,
      position: session.restore ? session.restore.position : matches && Number.isFinite(position()) ? Math.round(position() * 100) / 100 : 0,
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
      // At the very end of a track YouTube's API can report the length of its
      // own next autoplay item while still naming the current video. Adopting
      // it would hide the end of the track, and the handoff would be mistaken
      // for a stalled page. Keep the track's duration once it has played to
      // the end of it; earlier corrections (stale lengths at start) still apply.
      const current=duration();
      const leaked=session.lastDuration > 0 && Math.abs(current-session.lastDuration) >= 2 &&
        session.lastPosition >= session.lastDuration - 3;
      if (!leaked) session.lastDuration=current;
      session.lastPosition=position();
    }
  }, 200);
})();
