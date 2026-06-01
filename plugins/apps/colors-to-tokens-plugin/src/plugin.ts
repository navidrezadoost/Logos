import type { PluginMessageEvent, PluginUIEvent } from './model.js';

const defaultSize = {
  width: 410,
  height: 280,
};

logos.ui.open('COLORS TO TOKENS', `?theme=${logos.theme}`, {
  width: defaultSize.width,
  height: defaultSize.height,
});

logos.on('themechange', (theme) => {
  sendMessage({ type: 'theme', content: theme });
});

logos.ui.onMessage<PluginUIEvent>((message) => {
  if (message.type === 'get-colors') {
    const colors = logos.library.local.colors.filter(
      (color) => !color.gradient,
    );

    const fileName = logos.currentFile?.name ?? 'Untitled';

    sendMessage({
      type: 'set-colors',
      colors,
      fileName,
    });
  } else if (message.type === 'resize') {
    if (
      logos.ui.size?.width === defaultSize.width &&
      logos.ui.size?.height === defaultSize.height
    ) {
      resize(message.width, message.height);
    }
  } else if (message.type === 'reset') {
    resize(defaultSize.width, defaultSize.height);
  }
});

function resize(width: number, height: number) {
  if ('resize' in logos.ui) {
    logos.ui.resize(width, height);
  }
}

function sendMessage(message: PluginMessageEvent) {
  logos.ui.sendMessage(message);
}
