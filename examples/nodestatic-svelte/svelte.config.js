import adapter from "@sveltejs/adapter-static";

const config = {
  kit: {
    adapter: adapter({
      fallback: undefined,
      pages: "build",
      assets: "build",
    }),
  },
};

export default config;
