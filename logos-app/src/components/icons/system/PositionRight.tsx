import React from "react";
import type { SVGProps } from "react";

export interface PositionRightProps extends SVGProps<SVGSVGElement> {
  size?: number | string;
}

const PositionRight = React.forwardRef<SVGSVGElement, PositionRightProps>(
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
        <path fill="currentColor" d="M576 576L512 576L512 64L576 64L576 576zM448 128L448 288L64 288L64 128L448 128zM448 352L448 512L192 512L192 352L448 352z"/>
      </svg>
    );
  }
);

PositionRight.displayName = "PositionRight";

export default PositionRight;
