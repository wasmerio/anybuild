import { useEffect, useRef, useState } from "react";
import type { Player } from "asciinema-player";
import "asciinema-player/dist/bundle/asciinema-player.css";

const liveMarker = "site-live";
const liveTimestamp = 33.144;

export function UseDemo() {
  const sectionRef = useRef<HTMLElement>(null);
  const playerContainerRef = useRef<HTMLDivElement>(null);
  const [shouldPlay, setShouldPlay] = useState(false);
  const [isLive, setIsLive] = useState(false);

  useEffect(() => {
    const section = sectionRef.current;
    if (!section) return;

    const observer = new IntersectionObserver(
      ([entry]) => {
        if (entry?.isIntersecting) {
          setShouldPlay(true);
          observer.disconnect();
        }
      },
      { threshold: 0.3 },
    );

    observer.observe(section);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const container = playerContainerRef.current;
    if (!container || !shouldPlay) return;

    let disposed = false;
    let player: Player | undefined;
    let syncInterval: number | undefined;

    void import("asciinema-player").then(({ create }) => {
      if (disposed) return;

      player = create("/demo.cast", container, {
        autoPlay: true,
        preload: true,
        loop: true,
        cols: 80,
        rows: 22,
        fit: "width",
        controls: true,
        cursorMode: "hidden",
        terminalFontFamily:
          '"JetBrains Mono", "SFMono-Regular", Consolas, "Liberation Mono", monospace',
        terminalLineHeight: 1.25,
      });

      const syncPreview = () => {
        if (!player) return;

        const duration = player.getDuration();
        const time = player.getCurrentTime();
        setIsLive(time >= liveTimestamp && (duration === undefined || time < duration - 0.1));
      };

      player.addEventListener("marker", ({ label }) => {
        if (label === liveMarker) setIsLive(true);
      });
      player.addEventListener("playing", syncPreview);
      player.addEventListener("ended", () => setIsLive(false));
      syncInterval = window.setInterval(syncPreview, 200);
    });

    return () => {
      disposed = true;
      if (syncInterval !== undefined) window.clearInterval(syncInterval);
      player?.dispose();
    };
  }, [shouldPlay]);

  return (
    <section ref={sectionRef} id="use" className="mt-24 scroll-mt-12 text-center">
      <h2 className="text-[38px] font-bold tracking-[-0.03em] text-white sm:text-[46px]">
        Use anybuild
      </h2>

      <div className="use-demo">
        <div className="use-demo__terminal">
          <div className="use-demo__terminal-bar" aria-hidden="true">
            <span className="use-demo__window-controls">
              <i />
              <i />
              <i />
            </span>
            <span>anybuild — node-next</span>
            <span className="use-demo__terminal-spacer" />
          </div>
          <div
            ref={playerContainerRef}
            className="use-demo__player"
            aria-label="Anybuild building and starting a Next.js project"
          />
        </div>

        <div
          className={`use-demo__browser ${isLive ? "use-demo__browser--live" : ""}`}
          aria-hidden={!isLive}
        >
          <div className="use-demo__browser-window">
            <div className="use-demo__browser-bar">
              <span className="use-demo__window-controls" aria-hidden="true">
                <i />
                <i />
                <i />
              </span>
              <div className="use-demo__address">
                <span aria-hidden="true">⌁</span>
                127.0.0.1:8080
              </div>
              <span className="use-demo__live">
                <i />
                Live
              </span>
            </div>
            <div className="use-demo__page">
              <h3>Hello from Next.js on Anybuild</h3>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
