import React from "react";
import type { SVGProps } from "react";

export interface BoolSubtractProps extends SVGProps<SVGSVGElement> {
  size?: number | string;
}

const BoolSubtract = React.forwardRef<SVGSVGElement, BoolSubtractProps>(
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
        <path fill="currentColor" d="M576 576L224 576L224 416L64 416L64 64L416 64L416 224L576 224L576 576zM352 352L352 128L128 128L128 352L352 352z"/>
      </svg>
    );
  }
);

BoolSubtract.displayName = "BoolSubtract";

export default BoolSubtract;
