import type { PluginMessageEvent, PluginUIEvent } from './model.js';

logos.ui.open('CONTRAST PLUGIN', `?theme=${logos.theme}`, {
  width: 285,
  height: 525,
});

logos.ui.onMessage<PluginUIEvent>((message) => {
  if (message.type === 'ready') {
    sendMessage({
      type: 'init',
      content: {
        theme: logos.theme,
        selection: logos.selection,
      },
    });

    initEvents();
  }
});

logos.on('selectionchange', () => {
  const shapes = logos.selection;
  sendMessage({ type: 'selection', content: shapes });

  initEvents();
});

let listeners: symbol[] = [];

function initEvents() {
  listeners.forEach((listener) => {
    logos.off(listener);
  });

  listeners = logos.selection.map((shape) => {
    return logos.on(
      'shapechange',
      () => {
        const shapes = logos.selection;
        sendMessage({ type: 'selection', content: shapes });
      },
      { shapeId: shape.id },
    );
  });
}

logos.on('themechange', () => {
  const theme = logos.theme;
  sendMessage({ type: 'theme', content: theme });
});

function sendMessage(message: PluginMessageEvent) {
  logos.ui.sendMessage(message);
}
