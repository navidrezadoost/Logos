import React from "react";
import type { SVGProps } from "react";

export interface ArrowProps extends SVGProps<SVGSVGElement> {
  size?: number | string;
}

const Arrow = React.forwardRef<SVGSVGElement, ArrowProps>(
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
        <path fill="currentColor" d="M566.9 342.6L589.5 320L566.9 297.4L406.9 137.4L384.3 114.8L339 160.1C340.3 161.4 383 204.1 467 288.1L64.3 288.1L64.3 352.1L467 352.1C383 436.1 340.3 478.8 339 480.1L384.3 525.4L406.9 502.8L566.9 342.8z"/>
      </svg>
    );
  }
);

Arrow.displayName = "Arrow";

export default Arrow;
