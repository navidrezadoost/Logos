export default function () {
  function createRulerGuides(): void {
    const page = logos.currentPage;

    if (page) {
      page.addRulerGuide('horizontal', logos.viewport.center.x);
      page.addRulerGuide('vertical', logos.viewport.center.y);
    }
  }

  function removeRulerGuides(): void {
    const page = logos.currentPage;

    if (page) {
      page.removeRulerGuide(page.rulerGuides[0]);
    }
  }

  createRulerGuides();
  removeRulerGuides();
}
