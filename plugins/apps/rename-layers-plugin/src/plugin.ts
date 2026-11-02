import { PluginMessageEvent } from './app/model';

logos.ui.open('RENAME LAYER PLUGIN', `?theme=${logos.theme}`, {
  width: 290,
  height: 550,
});

logos.on('themechange', (theme) => {
  logos.ui.sendMessage({ type: 'theme', content: theme });
});

logos.on('shapechange', () => {
  resetSelection();
});

logos.ui.onMessage<PluginMessageEvent>((message) => {
  if (message.type === 'ready') {
    resetSelection();
  } else if (message.type === 'replace-text') {
    const blockId = logos.history.undoBlockBegin();

    const shapes = getShapes();
    const shapesToUpdate = shapes?.filter((shape) => {
      return shape.name.includes(message.content.search);
    });
    shapesToUpdate?.forEach((shape) => {
      shape.name = shape.name.replace(
        message.content.search,
        message.content.replace,
      );
    });
    updateReplaceTextPreview(message.content.search);

    logos.history.undoBlockFinish(blockId);
  } else if (message.type === 'preview-replace-text') {
    updateReplaceTextPreview(message.content.search);
  } else if (message.type === 'add-text') {
    const blockId = logos.history.undoBlockBegin();

    const currentNames = message.content.map((shape) => shape.current);
    const shapes = getShapes();
    const shapesToUpdate = shapes?.filter((shape) =>
      currentNames.includes(shape.name),
    );
    shapesToUpdate?.forEach((shape) => {
      const newText = message.content.find((it) => it.current === shape.name);
      return (shape.name = newText?.new ?? shape.name);
    });

    logos.history.undoBlockFinish(blockId);

    resetSelection();
  }
});

function getShapes() {
  return logos.selection.length
    ? logos.selection
    : logos.currentPage?.findShapes();
}

function resetSelection() {
  logos.ui.sendMessage({
    type: 'selection',
    content: {
      selection: getShapes(),
    },
  });
}

function updateReplaceTextPreview(search: string) {
  if (search) {
    const shapes = getShapes();
    const shapesToUpdate = shapes?.filter((shape) => {
      return shape.name.includes(search);
    });
    logos.ui.sendMessage({
      type: 'selection',
      content: {
        selection: shapesToUpdate,
      },
    });
  } else {
    resetSelection();
  }
}
