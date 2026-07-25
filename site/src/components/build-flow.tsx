import { useEffect, useRef } from "react";
import type { ReactNode } from "react";
import { Globe2, Monitor } from "lucide-react";

type FlowNode = {
  name: string;
  slug?: string;
  icon?: "local" | "static";
};

type Flow = {
  source: number;
  target: number;
  app: string;
  appIcon: keyof typeof frameworkIcons;
  artifact: keyof typeof artifacts;
  color: string;
  offset: number;
};

type Point = {
  x: number;
  y: number;
};

const providers: FlowNode[] = [
  { name: "Static Sites", icon: "static" },
  { name: "Node.js", slug: "nodedotjs" },
  { name: "Python", slug: "python" },
  { name: "PHP", slug: "php" },
];

const targets: FlowNode[] = [
  { name: "Local preview", icon: "local" },
  { name: "Cloudflare", slug: "cloudflare" },
  { name: "Wasmer", slug: "wasmer" },
  { name: "Vercel", slug: "vercel" },
  { name: "Fly.io", slug: "flydotio" },
];

const devicon = (name: string, variant = "original") =>
  `https://cdn.jsdelivr.net/gh/devicons/devicon@latest/icons/${name}/${name}-${variant}.svg`;

const frameworkIcons = {
  hugo: devicon("hugo"),
  docusaurus:
    "https://raw.githubusercontent.com/facebook/docusaurus/main/website/static/img/docusaurus.svg",
  materialformkdocs: "https://cdn.jsdelivr.net/gh/selfhst/icons/svg/mkdocs-light.svg",
  gatsby: devicon("gatsby"),
  nextdotjs: devicon("nextjs"),
  astro: "https://astro.build/assets/press/astro-icon-light-gradient.svg",
  vite: devicon("vitejs"),
  django: "/django-logo.svg",
  fastapi: "/fastapi-logo.svg",
  flask: "/flask-logo.svg",
  wordpress: "https://upload.wikimedia.org/wikipedia/commons/9/98/WordPress_blue_logo.svg",
  laravel: devicon("laravel"),
  symfony: "https://cdn.simpleicons.org/symfony/ffffff",
} as const;

const artifacts = {
  system: { label: "Local build", icon: null, color: "#AEB7C8" },
  wrangler: {
    label: "Worker",
    icon: "https://cdn.simpleicons.org/cloudflareworkers",
    color: "#F38020",
  },
  wasmer: {
    label: "Wasmer package",
    icon: "https://cdn.simpleicons.org/wasmer",
    color: "#654FF0",
  },
  docker: {
    label: "Docker container",
    icon: "https://cdn.simpleicons.org/docker",
    color: "#2496ED",
  },
} as const;

const flows: Flow[] = [
  {
    source: 0,
    target: 0,
    app: "Hugo",
    appIcon: "hugo",
    artifact: "system",
    color: "#FF4088",
    offset: 0,
  },
  {
    source: 0,
    target: 1,
    app: "Docusaurus",
    appIcon: "docusaurus",
    artifact: "wrangler",
    color: "#3ECC5F",
    offset: 0.077,
  },
  {
    source: 0,
    target: 2,
    app: "MkDocs",
    appIcon: "materialformkdocs",
    artifact: "wasmer",
    color: "#526CFE",
    offset: 0.154,
  },
  {
    source: 0,
    target: 3,
    app: "Gatsby",
    appIcon: "gatsby",
    artifact: "docker",
    color: "#663399",
    offset: 0.231,
  },
  {
    source: 1,
    target: 3,
    app: "Next.js",
    appIcon: "nextdotjs",
    artifact: "docker",
    color: "#FFFFFF",
    offset: 0.308,
  },
  {
    source: 1,
    target: 1,
    app: "Astro",
    appIcon: "astro",
    artifact: "wrangler",
    color: "#BC52EE",
    offset: 0.385,
  },
  {
    source: 1,
    target: 0,
    app: "Vite",
    appIcon: "vite",
    artifact: "system",
    color: "#646CFF",
    offset: 0.462,
  },
  {
    source: 2,
    target: 4,
    app: "Django",
    appIcon: "django",
    artifact: "docker",
    color: "#44B78B",
    offset: 0.539,
  },
  {
    source: 2,
    target: 2,
    app: "FastAPI",
    appIcon: "fastapi",
    artifact: "wasmer",
    color: "#009688",
    offset: 0.616,
  },
  {
    source: 2,
    target: 0,
    app: "Flask",
    appIcon: "flask",
    artifact: "system",
    color: "#FFFFFF",
    offset: 0.693,
  },
  {
    source: 3,
    target: 2,
    app: "WordPress",
    appIcon: "wordpress",
    artifact: "wasmer",
    color: "#21759B",
    offset: 0.77,
  },
  {
    source: 3,
    target: 4,
    app: "Laravel",
    appIcon: "laravel",
    artifact: "docker",
    color: "#FF2D20",
    offset: 0.847,
  },
  {
    source: 3,
    target: 3,
    app: "Symfony",
    appIcon: "symfony",
    artifact: "docker",
    color: "#FFFFFF",
    offset: 0.924,
  },
];

function bezierPoint(
  start: Point,
  controlA: Point,
  controlB: Point,
  end: Point,
  progress: number,
): Point {
  const inverse = 1 - progress;
  return {
    x:
      inverse ** 3 * start.x +
      3 * inverse ** 2 * progress * controlA.x +
      3 * inverse * progress ** 2 * controlB.x +
      progress ** 3 * end.x,
    y:
      inverse ** 3 * start.y +
      3 * inverse ** 2 * progress * controlA.y +
      3 * inverse * progress ** 2 * controlB.y +
      progress ** 3 * end.y,
  };
}

function controls(start: Point, end: Point): [Point, Point] {
  const distance = end.x - start.x;
  return [
    { x: start.x + distance * 0.48, y: start.y },
    { x: end.x - distance * 0.48, y: end.y },
  ];
}

function traceCurve(context: CanvasRenderingContext2D, start: Point, end: Point) {
  const [controlA, controlB] = controls(start, end);
  context.moveTo(start.x, start.y);
  context.bezierCurveTo(controlA.x, controlA.y, controlB.x, controlB.y, end.x, end.y);
}

function pointOnCurve(start: Point, end: Point, progress: number) {
  const [controlA, controlB] = controls(start, end);
  return bezierPoint(start, controlA, controlB, end, progress);
}

function roundedRect(
  context: CanvasRenderingContext2D,
  x: number,
  y: number,
  width: number,
  height: number,
  radius: number,
) {
  const edge = Math.min(radius, width / 2, height / 2);
  context.beginPath();
  context.moveTo(x + edge, y);
  context.arcTo(x + width, y, x + width, y + height, edge);
  context.arcTo(x + width, y + height, x, y + height, edge);
  context.arcTo(x, y + height, x, y, edge);
  context.arcTo(x, y, x + width, y, edge);
  context.closePath();
}

function FlowIcon({ node }: { node: FlowNode }) {
  if (node.icon === "local") {
    return <Monitor className="h-5 w-5 text-[#F7F9FC]" aria-hidden="true" />;
  }
  if (node.icon === "static") {
    return <Globe2 className="h-5 w-5 text-[#F7F9FC]" aria-hidden="true" />;
  }

  return (
    <img
      src={`https://cdn.simpleicons.org/${node.slug}/ffffff`}
      alt=""
      aria-hidden="true"
      className="h-5 w-5 opacity-90"
      loading="lazy"
    />
  );
}

export function BuildFlow({ logo }: { logo: ReactNode }) {
  const stageRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const stage = stageRef.current;
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d");
    if (!stage || !canvas || !context) return;

    let animationFrame = 0;
    let visible = true;
    let reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    let sourcePoints: Point[] = [];
    let targetPoints: Point[] = [];
    let centerInPoints: Point[] = [];
    let centerOutPoints: Point[] = [];
    let redraw = () => {};
    const iconImages = new Map<string, HTMLImageElement>();

    const loadIcon = (url: string) => {
      if (iconImages.has(url)) return;
      const image = new Image();
      image.onload = () => redraw();
      image.src = url;
      iconImages.set(url, image);
    };

    for (const flow of flows) {
      loadIcon(frameworkIcons[flow.appIcon]);
      const artifact = artifacts[flow.artifact];
      if (artifact.icon) loadIcon(artifact.icon);
    }

    const resize = () => {
      const stageRect = stage.getBoundingClientRect();
      const pixelRatio = Math.min(window.devicePixelRatio || 1, 2);
      canvas.width = Math.round(stageRect.width * pixelRatio);
      canvas.height = Math.round(stageRect.height * pixelRatio);
      context.setTransform(pixelRatio, 0, 0, pixelRatio, 0, 0);

      sourcePoints = Array.from(stage.querySelectorAll<HTMLElement>("[data-flow-source]")).map(
        (node) => {
          const rect = node.getBoundingClientRect();
          return {
            x: rect.right - stageRect.left,
            y: rect.top + rect.height / 2 - stageRect.top,
          };
        },
      );
      targetPoints = Array.from(stage.querySelectorAll<HTMLElement>("[data-flow-target]")).map(
        (node) => {
          const rect = node.getBoundingClientRect();
          return {
            x: rect.left - stageRect.left,
            y: rect.top + rect.height / 2 - stageRect.top,
          };
        },
      );

      const center = stage
        .querySelector<HTMLElement>("[data-flow-center]")
        ?.getBoundingClientRect();
      if (center) {
        const top = center.top - stageRect.top + center.height * 0.18;
        const verticalSpan = center.height * 0.64;
        const distribute = (count: number, x: number) =>
          Array.from({ length: count }, (_, index) => ({
            x,
            y: top + verticalSpan * (count === 1 ? 0.5 : index / (count - 1)),
          }));

        centerInPoints = distribute(sourcePoints.length, center.left - stageRect.left);
        centerOutPoints = distribute(targetPoints.length, center.right - stageRect.left);
      }
    };

    const drawMonitor = (point: Point, size: number, color: string) => {
      const width = size * 0.54;
      const height = size * 0.38;
      context.strokeStyle = color;
      context.lineWidth = 1.6;
      roundedRect(
        context,
        point.x - width / 2,
        point.y - height / 2 - size * 0.06,
        width,
        height,
        2,
      );
      context.stroke();
      context.beginPath();
      context.moveTo(point.x, point.y + height / 2 - size * 0.06);
      context.lineTo(point.x, point.y + height / 2 + size * 0.12);
      context.moveTo(point.x - size * 0.14, point.y + height / 2 + size * 0.12);
      context.lineTo(point.x + size * 0.14, point.y + height / 2 + size * 0.12);
      context.stroke();
    };

    const drawParticleIcon = (
      point: Point,
      iconUrl: string | null,
      color: string,
      alpha: number,
      conversion: number,
    ) => {
      const size = 22 + conversion * 6;
      const x = point.x - size / 2;
      const y = point.y - size / 2;

      context.save();
      context.globalAlpha = alpha;

      if (iconUrl) {
        const image = iconImages.get(iconUrl);
        if (image?.complete && image.naturalWidth > 0) {
          context.drawImage(image, x, y, size, size);
        }
      } else {
        drawMonitor(point, size, color);
      }
      context.restore();
    };

    const drawLabel = (point: Point, label: string, color: string, alpha: number) => {
      context.save();
      context.font = '500 12px "Inter", ui-sans-serif, system-ui, sans-serif';
      const width = context.measureText(label).width + 18;
      const x = point.x - width / 2;
      const y = point.y + 19;

      context.globalAlpha = alpha;
      roundedRect(context, x, y, width, 25, 8);
      context.fillStyle = "rgba(5, 9, 18, 0.9)";
      context.fill();
      context.fillStyle = color;
      context.textAlign = "center";
      context.textBaseline = "middle";
      context.fillText(label, point.x, y + 12.5);
      context.restore();
    };

    const draw = (time: number) => {
      const width = canvas.clientWidth;
      const height = canvas.clientHeight;
      context.clearRect(0, 0, width, height);

      context.save();
      context.lineWidth = 1;
      for (const [index, point] of sourcePoints.entries()) {
        const centerIn = centerInPoints[index];
        if (!centerIn) continue;
        const gradient = context.createLinearGradient(point.x, 0, centerIn.x, 0);
        gradient.addColorStop(0, "rgba(112, 121, 141, 0.12)");
        gradient.addColorStop(1, "rgba(119, 88, 255, 0.48)");
        context.strokeStyle = gradient;
        context.beginPath();
        traceCurve(context, point, centerIn);
        context.stroke();
      }
      for (const [index, point] of targetPoints.entries()) {
        const centerOut = centerOutPoints[index];
        if (!centerOut) continue;
        const gradient = context.createLinearGradient(centerOut.x, 0, point.x, 0);
        gradient.addColorStop(0, "rgba(119, 88, 255, 0.48)");
        gradient.addColorStop(1, "rgba(112, 121, 141, 0.12)");
        context.strokeStyle = gradient;
        context.beginPath();
        traceCurve(context, centerOut, point);
        context.stroke();
      }
      context.restore();

      for (const flow of flows) {
        const source = sourcePoints[flow.source];
        const target = targetPoints[flow.target];
        const centerIn = centerInPoints[flow.source];
        const centerOut = centerOutPoints[flow.target];
        if (!source || !target || !centerIn || !centerOut) continue;

        const artifact = artifacts[flow.artifact];
        const cycle = reducedMotion ? flow.offset : (time / 16000 + flow.offset) % 1;
        if (cycle > 0.56) continue;
        const progress = cycle / 0.56;
        let point: Point;
        let label: string;
        let icon: string | null;
        let color: string;
        let conversion = 0;

        if (progress < 0.45) {
          point = pointOnCurve(source, centerIn, progress / 0.45);
          label = flow.app;
          icon = frameworkIcons[flow.appIcon];
          color = flow.color;
        } else if (progress < 0.55) {
          point = {
            x: centerIn.x + (centerOut.x - centerIn.x) * ((progress - 0.45) / 0.1),
            y: centerIn.y + (centerOut.y - centerIn.y) * ((progress - 0.45) / 0.1),
          };
          const converted = progress >= 0.5;
          label = converted ? artifact.label : flow.app;
          icon = converted ? artifact.icon : frameworkIcons[flow.appIcon];
          color = converted ? artifact.color : flow.color;
          conversion = 1 - Math.abs(progress - 0.5) / 0.05;
        } else {
          point = pointOnCurve(centerOut, target, (progress - 0.55) / 0.45);
          label = artifact.label;
          icon = artifact.icon;
          color = artifact.color;
        }

        const fade = Math.min(progress / 0.05, (1 - progress) / 0.05, 1);
        drawParticleIcon(point, icon, color, fade, conversion);

        if (conversion < 0.72) {
          drawLabel(point, label, color, fade * (1 - conversion));
        }
      }

      if (!reducedMotion && visible) {
        animationFrame = requestAnimationFrame(draw);
      }
    };
    redraw = () => {
      if (reducedMotion) draw(performance.now());
    };

    const resizeObserver = new ResizeObserver(() => {
      resize();
      if (reducedMotion) draw(0);
    });
    const motionQuery = window.matchMedia("(prefers-reduced-motion: reduce)");
    const handleMotionChange = (event: MediaQueryListEvent) => {
      reducedMotion = event.matches;
      cancelAnimationFrame(animationFrame);
      draw(performance.now());
    };
    const visibilityObserver = new IntersectionObserver(([entry]) => {
      visible = entry.isIntersecting;
      cancelAnimationFrame(animationFrame);
      if (visible) animationFrame = requestAnimationFrame(draw);
    });

    resizeObserver.observe(stage);
    visibilityObserver.observe(stage);
    motionQuery.addEventListener("change", handleMotionChange);
    resize();
    animationFrame = requestAnimationFrame(draw);

    return () => {
      cancelAnimationFrame(animationFrame);
      resizeObserver.disconnect();
      visibilityObserver.disconnect();
      motionQuery.removeEventListener("change", handleMotionChange);
    };
  }, []);

  return (
    <section className="build-flow" aria-labelledby="build-flow-title">
      <div ref={stageRef} className="build-flow__stage">
        <canvas ref={canvasRef} className="build-flow__canvas" aria-hidden="true" />

        <div className="build-flow__column-label build-flow__column-label--left">Providers</div>
        <div className="build-flow__column-label build-flow__column-label--right">Deployments</div>

        {providers.map((provider, index) => (
          <div
            key={provider.name}
            data-flow-source
            className="build-flow__node build-flow__node--source"
            style={{ top: `${17 + index * 22}%` }}
          >
            <FlowIcon node={provider} />
            <span>{provider.name}</span>
          </div>
        ))}

        <div data-flow-center className="build-flow__center">
          <div className="build-flow__center-logo">{logo}</div>
          <span>Anybuild</span>
          <small>Detect · Build · Adapt</small>
        </div>

        {targets.map((target, index) => (
          <div
            key={target.name}
            data-flow-target
            className="build-flow__node build-flow__node--target"
            style={{ top: `${15 + index * 17.5}%` }}
          >
            <FlowIcon node={target} />
            <span>{target.name}</span>
          </div>
        ))}

        <p id="build-flow-title" className="sr-only">
          Static sites, Node.js, Python, and PHP projects flow through Anybuild. Frameworks
          including Hugo, Docusaurus, MkDocs, Gatsby, Next.js, Astro, Vite, Django, FastAPI, Flask,
          WordPress, Laravel, and Symfony become local previews, Wrangler deployments, Wasmer
          packages, or Docker containers for Cloudflare, Wasmer, Vercel, and Fly.io.
        </p>
      </div>
    </section>
  );
}
