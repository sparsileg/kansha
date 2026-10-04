// jsdom has no Web Animations API, which Svelte's transitions use (the
// account panel's slide, shell/motion.ts). This stand-in finishes each
// animation at once, so a sliding element comes and goes as before.
if (typeof Element !== "undefined" && !Element.prototype.animate) {
  Element.prototype.animate = function () {
    const animation = {
      onfinish: null as null | (() => void),
      currentTime: 0,
      effect: null,
      cancel() {},
      finish() {},
      play() {},
      pause() {},
    };
    queueMicrotask(() => animation.onfinish?.());
    return animation as unknown as Animation;
  };
}
