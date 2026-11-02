import React from "react";
import type { SVGProps } from "react";

export interface PositionBottomProps extends SVGProps<SVGSVGElement> {
  size?: number | string;
}

const PositionBottom = React.forwardRef<SVGSVGElement, PositionBottomProps>(
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
        <path fill="currentColor" d="M576 576L576 512L64 512L64 576L576 576zM128 448L288 448L288 64L128 64L128 448zM352 448L512 448L512 192L352 192L352 448z"/>
      </svg>
    );
  }
);

PositionBottom.displayName = "PositionBottom";

export default PositionBottom;
