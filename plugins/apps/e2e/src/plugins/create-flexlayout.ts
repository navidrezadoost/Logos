export default function () {
  function createFlexLayout(): void {
    const board = logos.createBoard();
    board.horizontalSizing = 'auto';
    board.verticalSizing = 'auto';

    board.x = logos.viewport.center.x;
    board.y = logos.viewport.center.y;

    const flex = board.addFlexLayout();

    flex.dir = 'column';
    flex.wrap = 'wrap';
    flex.alignItems = 'center';
    flex.justifyContent = 'center';
    flex.verticalPadding = 5;
    flex.horizontalPadding = 5;
    flex.horizontalSizing = 'fill';
    flex.verticalSizing = 'fill';

    board.appendChild(logos.createRectangle());
    board.appendChild(logos.createEllipse());
  }

  createFlexLayout();
}
