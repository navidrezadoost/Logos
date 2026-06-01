import React from "react";
import type { SVGProps } from "react";

export interface BoolUnionProps extends SVGProps<SVGSVGElement> {
  size?: number | string;
}

const BoolUnion = React.forwardRef<SVGSVGElement, BoolUnionProps>(
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
        <path fill="currentColor" d="M416 64L64 64L64 416L224 416L224 576L576 576L576 224L416 224L416 64z"/>
      </svg>
    );
  }
);

BoolUnion.displayName = "BoolUnion";

export default BoolUnion;
