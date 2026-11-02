import React from "react";
import type { SVGProps } from "react";

export interface PositionLeftProps extends SVGProps<SVGSVGElement> {
  size?: number | string;
}

const PositionLeft = React.forwardRef<SVGSVGElement, PositionLeftProps>(
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
        <path fill="currentColor" d="M64 576L128 576L128 64L64 64L64 576zM192 128L192 288L576 288L576 128L192 128zM192 352L192 512L448 512L448 352L192 352z"/>
      </svg>
    );
  }
);

PositionLeft.displayName = "PositionLeft";

export default PositionLeft;
