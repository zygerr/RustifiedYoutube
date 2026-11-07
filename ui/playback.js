function controlPlayback(action) {
  if (location.origin !== "https://music.youtube.com") return;
  const player = document.querySelector("ytmusic-player-bar");
  const video = document.querySelector("video");
  switch (action) {
    case "play_pause":
      if (video) {
        if (video.paused) video.play().catch(() => {});
        else video.pause();
      }
      break;
    case "next":
      player?.querySelector(".next-button")?.click();
      break;
    case "previous":
      player?.querySelector(".previous-button")?.click();
      break;
    case "mute":
      if (video) video.muted = !video.muted;
      break;
  }
}
