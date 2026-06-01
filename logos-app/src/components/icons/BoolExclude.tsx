import React from "react";
import type { SVGProps } from "react";

export interface BoolExcludeProps extends SVGProps<SVGSVGElement> {
  size?: number | string;
}

const BoolExclude = React.forwardRef<SVGSVGElement, BoolExcludeProps>(
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
        <path fill="currentColor" d="M64 64L416 64L416 224L576 224L576 576L224 576L224 416L64 416L64 64zM384 256L256 256L256 384L384 384L384 256z"/>
      </svg>
    );
  }
);

BoolExclude.displayName = "BoolExclude";

export default BoolExclude;
