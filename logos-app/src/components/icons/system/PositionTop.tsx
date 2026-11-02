import React from "react";
import type { SVGProps } from "react";

export interface PositionTopProps extends SVGProps<SVGSVGElement> {
  size?: number | string;
}

const PositionTop = React.forwardRef<SVGSVGElement, PositionTopProps>(
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
        <path fill="currentColor" d="M576 64L576 128L64 128L64 64L576 64zM128 192L288 192L288 576L128 576L128 192zM352 192L512 192L512 448L352 448L352 192z"/>
      </svg>
    );
  }
);

PositionTop.displayName = "PositionTop";

export default PositionTop;
