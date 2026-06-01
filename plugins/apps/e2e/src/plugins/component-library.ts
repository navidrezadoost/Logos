export default function () {
  const rectangle = logos.createRectangle();
  rectangle.x = logos.viewport.center.x;
  rectangle.y = logos.viewport.center.y;

  const shape = logos.currentPage?.getShapeById(rectangle.id);
  if (shape) {
    logos.library.local.createComponent([shape]);
  }
}
