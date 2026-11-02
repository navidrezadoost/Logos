import React from "react";
import type { SVGProps } from "react";

export interface TextProps extends SVGProps<SVGSVGElement> {
  size?: number | string;
}

const Text = React.forwardRef<SVGSVGElement, TextProps>(
  ({ size, className, style, ...props }, ref) => {
    const dimensions = size ? { width: size, height: size } : {
      width: props.width || 24,
      height: props.height || 24
    };

    return (
      <svg
        ref={ref}
        viewBox="0 0 640 640"
        xmlns="http://www.w3.org/2000/svg"
        width={dimensions.width}
        height={dimensions.height}
        fill={props.fill || "currentColor"}
        className={className}
        style={style}
        {...props}
      >
        <path fill="currentColor" d="M348.9 130.2L320 69.8L291.1 130.2L123.8 480L80 480L80 544L232 544L232 480L194.8 480L217.8 432L422.3 432L445.3 480L408.1 480L408.1 544L560.1 544L560.1 480L516.3 480L349 130.2zM391.7 368L248.3 368L320 218.2L391.7 368z"/>
      </svg>
    );
  }
);

Text.displayName = "Text";

export default Text;
