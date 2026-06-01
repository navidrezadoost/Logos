export default function () {
  function group() {
    const selected = logos.selection;

    if (selected.length && !logos.utils.types.isGroup(selected[0])) {
      return logos.group(selected);
    }
  }

  function ungroup() {
    const selected = logos.selection;

    if (selected.length && logos.utils.types.isGroup(selected[0])) {
      return logos.ungroup(selected[0]);
    }
  }

  const rectangle = logos.createRectangle();
  rectangle.x = logos.viewport.center.x;
  rectangle.y = logos.viewport.center.y;
  const rectangle2 = logos.createRectangle();
  rectangle2.x = logos.viewport.center.x + 100;
  rectangle2.y = logos.viewport.center.y + 100;

  logos.selection = [rectangle, rectangle2];

  group();
  ungroup();
}
