import type { Board, Rectangle, Text } from '@logos/plugin-types';

export default function () {
  function createText(text: string): Text | undefined {
    const textNode = logos.createText(text);

    if (!textNode) {
      return;
    }

    textNode.x = logos.viewport.center.x;
    textNode.y = logos.viewport.center.y;

    return textNode;
  }

  function createRectangle(): Rectangle {
    const rectangle = logos.createRectangle();

    rectangle.setPluginData('customKey', 'customValue');

    rectangle.x = logos.viewport.center.x;
    rectangle.y = logos.viewport.center.y;

    rectangle.resize(200, 200);

    return rectangle;
  }

  function createBoard(): Board {
    const board = logos.createBoard();

    board.name = 'Board name';

    board.x = logos.viewport.center.x;
    board.y = logos.viewport.center.y;

    board.borderRadius = 8;

    board.resize(300, 300);

    const text = logos.createText('Hello from board');

    if (!text) {
      throw new Error('Could not create text');
    }

    text.x = 10;
    text.y = 10;
    board.appendChild(text);

    return board;
  }

  createBoard();
  createRectangle();
  createText('Hello from plugin');
}
