import React from "react";
import type { SVGProps } from "react";

export interface RectProps extends SVGProps<SVGSVGElement> {
  size?: number | string;
}

const Rect = React.forwardRef<SVGSVGElement, RectProps>(
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
        <path fill="currentColor" d="M64 128L576 128L576 512L64 512L64 128z"/>
      </svg>
    );
  }
);

Rect.displayName = "Rect";

export default Rect;
