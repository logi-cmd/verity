const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;
const sections = [...document.querySelectorAll(".reveal")];
if (reduced || !("IntersectionObserver" in window)) sections.forEach((item) => item.classList.add("is-visible"));
else {
  const observer = new IntersectionObserver((entries) => entries.forEach((entry) => { if (entry.isIntersecting) { entry.target.classList.add("is-visible"); observer.unobserve(entry.target); } }), { threshold: 0.12 });
  sections.forEach((item) => observer.observe(item));
}
