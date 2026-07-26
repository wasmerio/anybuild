import { useEffect, useRef, useState, type CSSProperties } from "react";
import type { Player } from "asciinema-player";
import "asciinema-player/dist/bundle/asciinema-player.css";

const liveMarker = "site-live";
const liveDelayMs = 400;

type DemoId = "nextjs-preview" | "hugo-deploy";

const demos: Array<{
  id: DemoId;
  label: string;
  cast: string;
  liveTimestamp: number;
  terminalTitle: string;
  previewUrl: string;
}> = [
  {
    id: "nextjs-preview",
    label: "Preview Next.js locally with Wasmer",
    cast: "/demo.cast",
    liveTimestamp: 20.641,
    terminalTitle: "anybuild — node-next",
    previewUrl: "http://127.0.0.1:8080",
  },
  {
    id: "hugo-deploy",
    label: "Deploy Mkdocs documentation to Wasmer",
    cast: "/deploy-wasmer.cast",
    liveTimestamp: 24.406,
    terminalTitle: "anybuild — mkdocs",
    previewUrl: "https://mkdocs.wasmer.app",
  },
];

export function UseDemo() {
  const sectionRef = useRef<HTMLElement>(null);
  const playerContainerRef = useRef<HTMLDivElement>(null);
  const [selectedDemo, setSelectedDemo] = useState<DemoId>("nextjs-preview");
  const [shouldPlay, setShouldPlay] = useState(false);
  const [isLive, setIsLive] = useState(false);
  const [playbackProgress, setPlaybackProgress] = useState(0);
  const selectedDemoConfig = demos.find((demo) => demo.id === selectedDemo) ?? demos[0];

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

    setIsLive(false);
    setPlaybackProgress(0);

    let disposed = false;
    let player: Player | undefined;
    let syncInterval: number | undefined;
    let liveTimer: number | undefined;

    void import("asciinema-player").then(({ create }) => {
      if (disposed) return;

      player = create(selectedDemoConfig.cast, container, {
        autoPlay: true,
        preload: true,
        loop: false,
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
        setPlaybackProgress(
          duration === undefined || duration <= 0 ? 0 : Math.min(1, Math.max(0, time / duration)),
        );
        setIsLive(
          time >= selectedDemoConfig.liveTimestamp &&
            (duration === undefined || time < duration - 0.1),
        );
      };

      player.addEventListener("marker", ({ label }) => {
        if (label !== liveMarker) return;
        if (liveTimer !== undefined) window.clearTimeout(liveTimer);
        liveTimer = window.setTimeout(() => setIsLive(true), liveDelayMs);
      });
      player.addEventListener("playing", syncPreview);
      player.addEventListener("ended", () => {
        if (liveTimer !== undefined) window.clearTimeout(liveTimer);
        setIsLive(false);
        setPlaybackProgress(0);
        setSelectedDemo((currentDemo) =>
          currentDemo === "nextjs-preview" ? "hugo-deploy" : "nextjs-preview",
        );
      });
      syncInterval = window.setInterval(syncPreview, 50);
    });

    return () => {
      disposed = true;
      if (syncInterval !== undefined) window.clearInterval(syncInterval);
      if (liveTimer !== undefined) window.clearTimeout(liveTimer);
      player?.dispose();
    };
  }, [selectedDemo, selectedDemoConfig, shouldPlay]);

  return (
    <section ref={sectionRef} id="use" className="mt-24 scroll-mt-12 text-center">
      <h2 className="text-[38px] font-bold tracking-[-0.03em] text-white sm:text-[46px]">
        Use anybuild
      </h2>

      <div className="use-demo">
        <div className="use-demo__selector" role="tablist" aria-label="Choose an Anybuild demo">
          {demos.map((demo, index) => {
            const isSelected = selectedDemo === demo.id;

            return (
              <button
                key={demo.id}
                type="button"
                role="tab"
                id={`demo-tab-${demo.id}`}
                aria-controls="anybuild-demo-panel"
                aria-selected={isSelected}
                className={`use-demo__option ${isSelected ? "use-demo__option--selected" : ""}`}
                style={
                  isSelected
                    ? ({ "--demo-progress": playbackProgress } as CSSProperties)
                    : undefined
                }
                onClick={() => setSelectedDemo(demo.id)}
              >
                <span className="use-demo__option-number">0{index + 1}</span>
                <span>{demo.label}</span>
              </button>
            );
          })}
        </div>

        <div
          id="anybuild-demo-panel"
          className="use-demo__stage"
          role="tabpanel"
          aria-labelledby={`demo-tab-${selectedDemo}`}
        >
          <div className="use-demo__terminal">
            <div className="use-demo__terminal-bar" aria-hidden="true">
              <span className="use-demo__window-controls">
                <i />
                <i />
                <i />
              </span>
              <span>{selectedDemoConfig.terminalTitle}</span>
              <span className="use-demo__terminal-spacer" />
            </div>
            <div
              ref={playerContainerRef}
              className="use-demo__player"
              aria-label={
                selectedDemo === "nextjs-preview"
                  ? "Anybuild previewing a Next.js project locally with Wasmer"
                  : "Anybuild deploying an MkDocs site to Wasmer"
              }
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
                  {selectedDemoConfig.previewUrl}
                </div>
                <span className="use-demo__live">
                  <i />
                  Live
                </span>
              </div>
              <div className="use-demo__page">
                {selectedDemo === "hugo-deploy" ? (
                  <iframe
                    src={selectedDemoConfig.previewUrl}
                    title="Deployed MkDocs site on Wasmer"
                    loading="lazy"
                  />
                ) : (
                  <h3>Hello from Next.js on Anybuild</h3>
                )}
              </div>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
